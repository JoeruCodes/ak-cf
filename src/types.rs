use chrono::Utc;
use rand::{
    distributions::{Alphanumeric, DistString},
    thread_rng, Rng,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use worker::{console_log, Date};
use std::collections::HashMap;

use crate::notification::{Notification, Read};
use crate::{
    daily_task::{Links, SocialPlatform},
    notification::NotificationType,
};

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub enum Op {
    CombineAlien(usize, usize),
    SpawnAlien,
    DeleteAlienFromActive(usize),
    UsePowerup(usize, usize), //changed
    SpawnPowerup(PowerUpKind),
    GetData,
    Register(String),
    AwardBadge(BadgesKind),
    UpdateEmail(String),
    UpdatePfp(usize),
    UpdateIq(usize),
    IncrementAkaiBalance,
    DecrementAkaiBalance,
    MoveAlienFromInventoryToActive,
    UpdateUserName(Option<String>),
    UpdatePassword(String),
    MoveAlienInGrid(usize, usize),
    AddNotificationInternal(Notification),
    MarkNotificationRead(String),
    UseReferralCode(String),
    UpdateDbFromDo,
    GenerateDailyTasks,
    CheckDailyTask(Option<String>),
    ClaimDailyReward(usize),
    SyncData,
    SubmitMcqAnswers(String, Vec<String>), // (datapoint_id, answers)
    SubmitTextAnswer(String, usize, String), // (datapoint_id, idx, text)
}

#[derive(Serialize, Deserialize, Debug)]
pub struct WsMsg {
    pub op: Op,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct DurableObjectAugmentedMsg {
    pub user_id: String,
    pub op: Op,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Copy)]
pub enum PowerUpKind {
    RowPowerUp,
    ColumnPowerUp,
    NearestSquarePowerUp,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub enum BadgesKind {
    TenTaskBadge,
    TwentyTaskBadge,
    ThirtyTaskBadge,
}

#[derive(Deserialize, Clone, Debug, Serialize, PartialEq)]
pub enum LeagueType {
    Bronze,
    Silver,
    Gold,
    Platinum,
    Diamond,
    Master,
    GrandMaster,
    Challenger,
}

impl LeagueType {
    pub fn from_product(product: usize) -> Self {
        match product / 50 {
            0 => LeagueType::Bronze,
            1 => LeagueType::Silver,
            2 => LeagueType::Gold,
            3 => LeagueType::Platinum,
            4 => LeagueType::Diamond,
            5 => LeagueType::Master,
            6 => LeagueType::GrandMaster,
            _ => LeagueType::Challenger,
        }
    }
}

#[derive(Deserialize, Clone, Debug, Serialize)]
pub struct LeaderboardData {
    pub league: usize,
    pub global: usize,
}

#[derive(Deserialize, Clone, Debug, Serialize)]
pub struct UserProfile {
    pub user_id: String,
    pub email: Option<String>,
    pub pfp: usize,
    pub user_name: Option<String>,
    pub password: Option<String>,
    pub last_login: u64,
    pub real_login:u64,
}

#[derive(Deserialize, Clone, Debug, Serialize)]
pub struct GameState {
    pub active_aliens: [usize; 16],
    pub inventory_aliens: usize,
    pub power_ups: Vec<PowerUpKind>,
    pub king_lvl: usize,
    pub total_merged_aliens: usize,
}

#[derive(Deserialize, Clone, Debug, Serialize)]
pub struct Progress {
    pub iq: usize,
    pub social_score: usize,
    pub product: usize,
    pub all_task_done: bool,
    pub akai_balance: usize,
    pub total_task_completed: usize,
    pub streak: usize,
    pub badges: Vec<BadgesKind>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[allow(non_snake_case)]
pub struct Question {
    pub q: String,
    pub a: String,
    pub textAnswers: Vec<String>,
    pub mcqAnswers: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct McqPreLabel {
    pub map_placement: String,
    pub questions: Vec<Question>,
    pub summary: String,
    pub keywords: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[allow(non_snake_case)]
pub struct McqVideoTask {
    #[serde(rename = "_id")]
    pub id: String,
    pub task_id: String,
    pub mediaUrl: String,
    pub preLabel: McqPreLabel,
    pub visited: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[allow(non_snake_case)]
pub struct TextVideoTask {
    pub datapointId: String,
    pub questionIndex: usize,
    pub question: String,
    pub mediaUrl: String,
    pub visited: bool,
    pub map_placement: String,
    pub keywords: Vec<String>,
}

#[derive(Deserialize, Clone, Debug, Serialize)]
pub struct DailyProgress {
    pub links: Vec<Links>,
    pub daily_merge: (usize, usize, bool),
    pub daily_annotate: (usize, usize, bool),
    pub daily_powerups: (usize, usize, bool),
    pub total_completed: usize,
    pub alien_earned: Option<usize>,
    pub pu_earned: Option<PowerUpKind>,
    pub mcq_video_tasks: Vec<McqVideoTask>,
    pub text_video_tasks: Vec<TextVideoTask>,
    pub last_task_generation: u64,
}

#[derive(Deserialize, Clone, Debug, Serialize)]
pub struct SocialData {
    pub players_referred: usize,
    pub referal_code: String,
}

#[derive(Deserialize, Clone, Debug, Serialize)]
pub struct UserData {
    pub profile: UserProfile,
    pub game_state: GameState,
    pub progress: Progress,
    pub social: SocialData,
    pub league: LeagueType,
    pub notifications: Vec<Notification>, // <-- added this
    pub daily: DailyProgress,
}

impl Default for UserData {
    fn default() -> Self {
        console_log!("defaulting user data");
        let mut res = Self {
            profile: UserProfile {
                user_id: "kunal".to_string(),
                email: None,
                pfp: 1,
                user_name: None,
                password: Some("123456".to_string()),
                last_login: Date::now().as_millis() / 1000,
                real_login: Date::now().as_millis() / 1000,
            },
            game_state: GameState {
                active_aliens: [0; 16],
                inventory_aliens: 10,
                power_ups: Vec::new(),
                king_lvl: 1,
                total_merged_aliens: 0,
            },
            progress: Progress {
                iq: 50,
                social_score: 0,
                all_task_done: false,
                product: 0,
                akai_balance: 0,
                total_task_completed: 0,
                streak: 0,
                badges: Vec::new(),
            },
            social: SocialData {
                players_referred: 0,
                referal_code: thread_rng()
                    .sample_iter(Alphanumeric)
                    .take(8)
                    .map(|b| b as char)
                    .collect(),
            },
            league: LeagueType::Bronze,
            notifications: Vec::new(), // <-- added this,
            daily: DailyProgress {
                links: Vec::new(),
                mcq_video_tasks: Vec::new(),
                text_video_tasks: Vec::new(),
                daily_merge: (0, 0, false),
                daily_annotate: (0, 0, false),
                daily_powerups: (0, 0, false),
                total_completed: 0,
                alien_earned: None,
                pu_earned: None,
                last_task_generation: 0,
            },
        };

        for i in 0..5 {
            res.game_state.active_aliens[i] = 1;
        }

        res.notifications.push(Notification {
            notification_id: Uuid::new_v4().to_string(),
            user_id: res.profile.user_id.clone(),
            notification_type: NotificationType::System,
            message: "Welcome to the game!".to_string(),
            timestamp: Utc::now().timestamp(),
            read: Read::No,
            metadata: None, // 👈 No metadata
        });

        res
    }
}

impl UserData {
    pub fn calculate_last_login(&mut self) {
        // Streak Calculation Logic
        console_log!("calculating streak");
        let current_time = Date::now().as_millis() / 1000;
        let time_since_last_login = current_time - self.profile.last_login;
        let one_day = 60 * 60 * 24;
        let two_days = one_day * 2;

        if time_since_last_login > one_day && time_since_last_login < two_days {
            self.progress.streak += 1;
            self.profile.last_login = current_time;
        } else if time_since_last_login >= two_days {
            self.progress.streak = 0;
            self.profile.last_login = current_time;
        }

    }
}

