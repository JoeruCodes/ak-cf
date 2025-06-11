const { default: mongoose } = require("mongoose");
const Datapoint = require("../models/datapoint");
const Redis = require("ioredis");
const {
  initializeMCQPriorityQ,
  calcTxtPriorityScore,
} = require("../utils/lib");

const redis = new Redis(process.env.REDIS_URL);

// Create a separate subscriber connection for keyspace notifications
const subscriber = new Redis(process.env.REDIS_URL);

// Enable keyspace notifications for expired events
// redis.config("SET", "notify-keyspace-events", "Ex");

// Subscribe to expiry events
subscriber.subscribe("__keyevent@0__:expired", (err) => {
  if (err) {
    console.error("Failed to subscribe to expiry events:", err);
  } else {
    console.log("Subscribed to Redis key expiry events");
  }
});

const MCQ_PRIORITY_QUEUE_KEY = "datapoints:mcq_priority_queue";
const TXT_PRIORITY_QUEUE_KEY = "datapoints:txt_priority_queue";
const ASSIGNED_DATAPOINT_EXPIRY = 60;
const ASSIGNMENT_MCQ_PREFIX = "assignment:mcq:";
const ASSIGNMENT_TXT_PREFIX = "assignment:txt:";

const fetchMcqQuestions = async (req, res) => {
  try {
    const { numberOfDatapoints } = req.body;
    if (!numberOfDatapoints) {
      return res
        .status(400)
        .json({ success: false, message: "Number of datapoints is missing!" });
    }
    const qualifiedIds = [];
    const pipeline = redis.pipeline();

    const allEntries = await redis.zrevrange(
      MCQ_PRIORITY_QUEUE_KEY,
      0,
      -1,
      "WITHSCORES"
    );

    if (allEntries.length === 0) {
      return res
        .status(500)
        .json({ success: false, message: "Priority Queue is empty!" });
    }

    for (
      let i = 0;
      i < allEntries.length && qualifiedIds.length < numberOfDatapoints;
      i += 2
    ) {
      const entry = JSON.parse(allEntries[i]);
      const score = parseFloat(allEntries[i + 1]);
      if (entry.users < 3) {
        const updatedEntry = {
          ...entry,
          users: entry.users + 1,
        };

        pipeline.zrem(MCQ_PRIORITY_QUEUE_KEY, allEntries[i]);
        pipeline.zadd(
          MCQ_PRIORITY_QUEUE_KEY,
          score,
          JSON.stringify(updatedEntry)
        );

        qualifiedIds.push(entry.id);

        pipeline.setex(
          `${ASSIGNMENT_MCQ_PREFIX}${entry.id}`, // Key format
          ASSIGNED_DATAPOINT_EXPIRY, // Same expiry
          ASSIGNED_DATAPOINT_EXPIRY
        );
      }
    }

    const results = await pipeline.exec();

    //check for errors in Redis
    const errors = results.filter((r) => r[0] instanceof Error);
    if (errors.length) {
      throw new AggregateError(errors, "Redis command errors");
    }

    const datapointIds = qualifiedIds.map(
      (id) => new mongoose.Types.ObjectId(id)
    );

    const datapoints = await Datapoint.find({ _id: { $in: datapointIds } });

    res.status(200).json({ success: true, datapoints: datapoints });
  } catch (error) {
    console.error("Failed to send Datapoints! ", error);
    res.status(500).json({
      success: false,
      error: error.name,
      message: error.message,
    });
  }
};

const addMcqAnswers = async (req, res) => {
  try {
    const { datapointId, answerObj, playerId } = req.body;

    if (!datapointId || !answerObj || !playerId) {
      return res.status(400).json({
        success: false,
        message: "Datapoint ID, answerObj, or playerId is missing!",
      });
    }

    // Prepare the update operations for each question
    const updateOperations = Object.entries(answerObj).map(
      ([questionIndex, answerText]) => ({
        updateOne: {
          filter: {
            _id: datapointId,
            [`preLabel.questions.${questionIndex}`]: { $exists: true }, // Check if question exists at this index
          },
          update: {
            $push: {
              [`preLabel.questions.${questionIndex}.mcqAnswers`]: {
                text: answerText,
                playerId: playerId,
              },
            },
          },
        },
      })
    );

    // Perform bulk write operation to update all questions
    const result = await Datapoint.bulkWrite(updateOperations);

    if (result.modifiedCount === 0) {
      return res.status(400).json({
        success: false,
        message: "No matching questions found to update!",
      });
    }

    const datapoint = await Datapoint.findById(datapointId);
    const mcqAnsLength = datapoint.preLabel.questions[0].mcqAnswers.length;

    let flaggedQuesIdxs = [];
    if (mcqAnsLength >= 3) {
      const questions = datapoint.preLabel.questions;

      flaggedQuesIdxs = questions
        .map((q, index) => ({
          index,
          mcqAnswers: q.mcqAnswers.map((ans) => ans.text), // Extract just the text for filtering
        }))
        .filter(({ mcqAnswers }) => {
          const yesCount = mcqAnswers.filter((ans) => ans === "Yes").length;
          return yesCount === 1 || yesCount === 0;
        })
        .map(({ index }) => index);

      const pipeline = redis.pipeline();

      if (flaggedQuesIdxs.length > 0) {
        flaggedQuesIdxs.forEach((i) => {
          const textAnsLength = 0;
          pipeline.zadd(
            TXT_PRIORITY_QUEUE_KEY,
            calcTxtPriorityScore(textAnsLength),
            JSON.stringify({
              id: datapointId,
              idx: i,
              users: 0,
            })
          );
        });

        // Then, update the MongoDB document to set isFlagged to true for these questions
        const updateFlagOperations = flaggedQuesIdxs.map((index) => ({
          updateOne: {
            filter: {
              _id: datapointId,
              [`preLabel.questions.${index}.q`]: { $exists: true }, // Ensure the question exists at this index
            },
            update: {
              $set: {
                [`preLabel.questions.${index}.isFlagged`]: true,
                processingStatus: "live-label-txt",
              },
            },
          },
        }));

        // Execute the flag updates
        await Datapoint.bulkWrite(updateFlagOperations);
      } else {
        await Datapoint.findByIdAndUpdate(datapointId, {
          processingStatus: "consensus",
        });

        await fetch(
          `https://consensus-api-git-main-akai-space.vercel.app/generate-prelabel-consensus`,
          {
            method: "POST",
            headers: {
              "Content-Type": "application/json",
            },
            body: JSON.stringify({
              datapoint_id: datapointId,
            }),
          }
        );
      }

      const results = await pipeline.exec();

      // Check for errors in Redis
      const errors = results.filter((r) => r[0] instanceof Error);
      if (errors.length) {
        throw new AggregateError(errors, "Redis command errors");
      }
    }

    // Update Redis priority queue
    const allEntries = await redis.zrevrange(
      MCQ_PRIORITY_QUEUE_KEY,
      0,
      -1,
      "WITHSCORES"
    );

    for (let i = 0; i < allEntries.length; i += 2) {
      const entry = JSON.parse(allEntries[i]);
      const score = parseFloat(allEntries[i + 1]);
      if (entry.id === datapointId) {
        const newEntry = {
          ...entry,
          users: Math.max(0, entry.users - 1),
        };

        const newEntryString = JSON.stringify(newEntry);

        // Atomic update
        await redis
          .multi()
          .zrem(MCQ_PRIORITY_QUEUE_KEY, allEntries[i])
          .zadd(MCQ_PRIORITY_QUEUE_KEY, score, newEntryString)
          .exec();

        if (entry.users === 1) {
          await redis.del(`${ASSIGNMENT_MCQ_PREFIX}${datapointId}`);
        }
      }
    }

    res.status(200).json({
      success: true,
      message: "MCQ Answers added successfully!",
    });
  } catch (error) {
    console.error("Failed to add MCQ answers: ", error);
    res.status(500).json({
      success: false,
      error: error.name,
      message: error.message,
    });
  }
};

const sendTxtQuestions = async (req, res) => {
  try {
    const { noOfQues } = req.body;
    if (!noOfQues) {
      return res.status(400).json({
        success: false,
        message: "Number of questions is missing!",
      });
    }

    const qualifiedEntries = [];
    const pipeline = redis.pipeline();

    // Get all entries from the priority queue (sorted by score ascending)
    const allEntries = await redis.zrange(
      TXT_PRIORITY_QUEUE_KEY,
      0,
      -1,
      "WITHSCORES"
    );

    if (allEntries.length === 0) {
      return res.status(404).json({
        success: false,
        message: "No text questions available in the queue",
      });
    }

    // Process entries until we have enough qualified questions
    for (
      let i = 0;
      i < allEntries.length && qualifiedEntries.length < noOfQues;
      i += 2
    ) {
      const entryStr = allEntries[i];
      const score = parseFloat(allEntries[i + 1]);
      const entry = JSON.parse(entryStr);

      if (entry.users < 3) {
        // Update the entry with incremented users count
        const updatedEntry = {
          ...entry,
          users: entry.users + 1,
        };

        // Queue Redis operations
        pipeline.zrem(TXT_PRIORITY_QUEUE_KEY, entryStr);
        pipeline.zadd(
          TXT_PRIORITY_QUEUE_KEY,
          score,
          JSON.stringify(updatedEntry)
        );
        pipeline.setex(
          `${ASSIGNMENT_TXT_PREFIX}${entry.id}:${entry.idx}`,
          ASSIGNED_DATAPOINT_EXPIRY,
          ASSIGNED_DATAPOINT_EXPIRY
        );

        const datapoint = await Datapoint.findById(entry.id);
        const question = datapoint.preLabel.questions[entry.idx].q;

        qualifiedEntries.push({
          datapointId: entry.id,
          questionIndex: entry.idx,
          question: question,
          keywords: datapoint.preLabel.keywords,
          map_placement: datapoint.preLabel.map_placement,
          mediaUrl: datapoint.mediaUrl,
        });
      }
    }

    // Execute all Redis commands atomically
    const results = await pipeline.exec();

    // Check for Redis errors
    const errors = results.filter((r) => r[0] instanceof Error);
    if (errors.length) {
      throw new AggregateError(errors, "Redis command errors");
    }

    if (qualifiedEntries.length === 0) {
      return res.status(404).json({
        success: false,
        message: "No available text questions with users < 3",
      });
    }

    res.status(200).json({
      success: true,
      questions: qualifiedEntries,
    });
  } catch (error) {
    console.error("Failed to send text questions:", error);
    res.status(500).json({
      success: false,
      error: error.name,
      message: error.message,
    });
  }
};

const addTxtAnswers = async (req, res) => {
  try {
    const { datapointId, idx, text, playerId } = req.body;

    if (!datapointId || idx === undefined || !text || !playerId) {
      return res.status(400).json({
        success: false,
        message: "Datapoint ID, question index, text, or playerId is missing!",
      });
    }

    // 1. Update MongoDB - add text answer to the specified question
    const updatedDatapoint = await Datapoint.findOneAndUpdate(
      {
        _id: datapointId,
        [`preLabel.questions.${idx}`]: { $exists: true },
      },
      {
        $push: {
          [`preLabel.questions.${idx}.textAnswers`]: {
            text,
            playerId,
          },
        },
      },
      {
        new: true, // 👈 return the updated document
      }
    );

    if (!updatedDatapoint) {
      return res.status(404).json({
        success: false,
        message: "Datapoint or question not found",
      });
    }

    // Check only one question whose index is given
    const isFullyLabeled =
      updatedDatapoint.preLabel.questions[idx].textAnswers.length >= 3;

    if (isFullyLabeled) {
      await Datapoint.findByIdAndUpdate(datapointId, {
        $set: {
          [`preLabel.questions.${idx}.isFlagged`]: false,
        },
      });
    }

    // Check the datapoint if all flagged questions have been labeled.
    const flaggedQues = updatedDatapoint.preLabel.questions.filter(
      (q) => q.isFlagged === true
    );

    if (flaggedQues.length > 0) {
      // Total textAnswers across all flagged questions
      let totalTextAnswers = 0;

      flaggedQues.forEach((q) => {
        totalTextAnswers += q.textAnswers?.length || 0;
      });
      if (totalTextAnswers >= flaggedQues.length * 3) {
        await Datapoint.findByIdAndUpdate(datapointId, {
          processingStatus: "consensus",
        });
        await fetch(
          `https://consensus-api-git-main-akai-space.vercel.app/generate-prelabel-consensus`,
          {
            method: "POST",
            headers: {
              "Content-Type": "application/json",
            },
            body: JSON.stringify({
              datapoint_id: datapointId,
            }),
          }
        );
      }
    }

    // 2. Update Redis - decrement users count for this question
    const allEntries = await redis.zrange(
      TXT_PRIORITY_QUEUE_KEY,
      0,
      -1,
      "WITHSCORES"
    );

    for (let i = 0; i < allEntries.length; i += 2) {
      const entryStr = allEntries[i];
      const score = parseFloat(allEntries[i + 1]);
      const entry = JSON.parse(entryStr);

      if (entry.id === datapointId && entry.idx === idx) {
        const newEntry = {
          ...entry,
          users: Math.max(0, entry.users - 1), // Decrement but don't go below 0
        };

        // Atomic update in Redis
        await redis
          .multi()
          .zrem(TXT_PRIORITY_QUEUE_KEY, entryStr)
          .zadd(TXT_PRIORITY_QUEUE_KEY, score, JSON.stringify(newEntry))
          .exec();

        if (entry.users == 1) {
          await redis.del(`${ASSIGNMENT_TXT_PREFIX}${datapointId}:${idx}`);
        }

        return res.status(200).json({
          success: true,
          message: "Text answer added successfully",
        });
      }
    }

    // If we get here, the entry wasn't found in Redis
    return res.status(404).json({
      success: false,
      message: "Question not found in priority queue",
    });
  } catch (error) {
    console.error("Failed to add text answer:", error);
    res.status(500).json({
      success: false,
      error: error.name,
      message: error.message,
    });
  }
};

//Refresh Priority Queue timeout after every 10 mins
setInterval(() => {
  initializeMCQPriorityQ();
}, 600000);

subscriber.on("message", async (channel, expiredKey) => {
  if (
    channel === "__keyevent@0__:expired" &&
    expiredKey.startsWith(ASSIGNMENT_MCQ_PREFIX)
  ) {
    try {
      const datapointID = expiredKey.split(":")[2];
      console.log(datapointID);

      const allEntries = await redis.zrevrange(
        MCQ_PRIORITY_QUEUE_KEY,
        0,
        -1,
        "WITHSCORES"
      );

      for (let i = 0; i < allEntries.length; i += 2) {
        const entry = JSON.parse(allEntries[i]);
        const score = parseFloat(allEntries[i + 1]);
        if (entry.id === datapointID) {
          const newEntry = {
            ...entry,
            users: 0,
          };

          const newEntryString = JSON.stringify(newEntry);

          // Atomic update
          await redis
            .multi()
            .zrem(MCQ_PRIORITY_QUEUE_KEY, allEntries[i]) // Remove current
            .zadd(MCQ_PRIORITY_QUEUE_KEY, score, newEntryString) // Add updated
            .exec();
        }
      }
    } catch (error) {
      console.error("Error handling expiry:", error);
    }
  }
});

subscriber.on("message", async (channel, expiredKey) => {
  if (
    channel === "__keyevent@0__:expired" &&
    expiredKey.startsWith(ASSIGNMENT_TXT_PREFIX)
  ) {
    try {
      const datapointID = expiredKey.split(":")[2];
      console.log(datapointID);

      const allEntries = await redis.zrange(
        TXT_PRIORITY_QUEUE_KEY,
        0,
        -1,
        "WITHSCORES"
      );

      for (let i = 0; i < allEntries.length; i += 2) {
        const entry = JSON.parse(allEntries[i]);
        const score = parseFloat(allEntries[i + 1]);
        if (entry.id === datapointID) {
          const newEntry = {
            ...entry,
            users: 0,
          };

          const newEntryString = JSON.stringify(newEntry);

          // Atomic update
          await redis
            .multi()
            .zrem(TXT_PRIORITY_QUEUE_KEY, allEntries[i]) // Remove current
            .zadd(TXT_PRIORITY_QUEUE_KEY, score, newEntryString) // Add updated
            .exec();
        }
      }
    } catch (error) {
      console.error("Error handling expiry:", error);
    }
  }
});

module.exports = {
  fetchMcqQuestions,
  addMcqAnswers,
  sendTxtQuestions,
  addTxtAnswers,
}; 