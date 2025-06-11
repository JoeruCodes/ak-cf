use crate::daily_task::Links;
use crate::gpt_voice::*;
use crate::notification::{push_notification_to_user_do, NotificationType};
use crate::types::DurableObjectAugmentedMsg;
use crate::utils::{
    fetch_mcq_video_tasks, fetch_text_video_tasks, find_user_id_by_referral_code,
    give_daily_reward, BASE_URL,
};
use crate::{daily_task::*, gpt_voice};
use rand::Rng;
use serde_json::json;
use sha2::digest::Update;
use sha2::Digest;
use std::collections::HashMap;
use worker::*;
use worker::{console_error, console_log, D1Database, Date, Env, Response, Result};

use crate::{
    sql::insert_new_user,
    types::{Op, PowerUpKind, UserData, WsMsg},
    utils::calculate_king_alien_lvl,
    utils::calculate_product,
};

use crate::notification::Read;

impl UserData {
    pub async fn resolve_op(
        &mut self,
        op_request: &DurableObjectAugmentedMsg,
        d1: &D1Database,
        env: &Env,
    ) -> Result<Response> {
        match &op_request.op {
            Op::CombineAlien(idx_a, idx_b) => {
                if idx_a == idx_b {
                    return Response::error("Combined Alien IDs cannot be the same", 400);
                }
                self.game_state.active_aliens[*idx_a] += 1;
                // Replace the second alien with (king_lvl-1)*10 + 1 if we have inventory
                self.game_state.active_aliens[*idx_b] = if self.game_state.inventory_aliens > 0 {
                    self.game_state.inventory_aliens -= 1;
                    (self.game_state.king_lvl - 1) * 10 + 1
                } else {
                    0
                };
                self.game_state.total_merged_aliens += 1;
                //daily task check
                self.daily.daily_merge.0 += 1;
                if (self.daily.daily_merge.0 == self.daily.daily_merge.1) {
                    self.daily.daily_merge.2 = true;
                    self.daily.total_completed += 1;
                    self.progress.social_score += 2;
                }
                calculate_king_alien_lvl(self);
                Response::ok(
                    json!({
                        "active_aliens": self.game_state.active_aliens,
                        "inventory_aliens": self.game_state.inventory_aliens,
                        "total_merged_aliens": self.game_state.total_merged_aliens,
                        "king_lvl": self.game_state.king_lvl,
                        "product": self.progress.product,
                        "links": self.daily.links,
                        "daily_merge": self.daily.daily_merge,
                        "daily_annotate": self.daily.daily_annotate,
                        "daily_powerups": self.daily.daily_powerups,
                        "total_completed": self.daily.total_completed,
                        "alien_earned": self.daily.alien_earned,
                        "pu_earned": self.daily.pu_earned
                    })
                    .to_string(),
                )
            }
            Op::SpawnAlien => {
                // Always add to inventory
                self.game_state.inventory_aliens += 1;

                Response::ok(
                    json!({
                        "active_aliens": self.game_state.active_aliens,
                        "inventory_aliens": self.game_state.inventory_aliens,
                        "total_merged_aliens": self.game_state.total_merged_aliens,
                        "king_lvl": self.game_state.king_lvl,
                        "product": self.progress.product
                    })
                    .to_string(),
                )
            }
            Op::MoveAlienFromInventoryToActive => {
                if self.game_state.inventory_aliens == 0 {
                    return Response::error("No aliens in inventory", 404);
                }

                if let Some(empty_slot) = self.game_state.active_aliens.iter().position(|a| *a == 0)
                {
                    // Decrease inventory count
                    self.game_state.inventory_aliens -= 1;

                    // Place (king_lvl-1)*10 + 1 in the empty slot
                    self.game_state.active_aliens[empty_slot] =
                        (self.game_state.king_lvl - 1) * 10 + 1;

                    calculate_king_alien_lvl(self);

                    Response::ok(
                        json!({
                            "active_aliens": self.game_state.active_aliens,
                            "inventory_aliens": self.game_state.inventory_aliens,
                            "king_lvl": self.game_state.king_lvl,
                            "product": self.progress.product,
                        })
                        .to_string(),
                    )
                } else {
                    Response::error("Active aliens grid is full!", 404)
                }
            }

            Op::SpawnPowerup(powerup) => {
                self.game_state.power_ups.push(*powerup);
                Response::ok(
                    json!({
                        "power_ups": self.game_state.power_ups
                    })
                    .to_string(),
                )
            }
            Op::UsePowerup(idx, target_pos) => {
                if *idx >= self.game_state.power_ups.len() || *target_pos >= 16 {
                    return Response::error("Invalid powerup index or target position", 400);
                }

                let power_up = self.game_state.power_ups.swap_remove(*idx);

                match power_up {
                    PowerUpKind::ColumnPowerUp => {
                        let col = *target_pos % 4;
                        for row in 0..4 {
                            let index = row * 4 + col;
                            if self.game_state.active_aliens[index] > 0 {
                                self.game_state.active_aliens[index] += 1;
                            }
                        }
                    }
                    PowerUpKind::RowPowerUp => {
                        let row = *target_pos / 4;
                        for col in 0..4 {
                            let index = row * 4 + col;
                            if self.game_state.active_aliens[index] > 0 {
                                self.game_state.active_aliens[index] += 1;
                            }
                        }
                    }
                    PowerUpKind::NearestSquarePowerUp => {
                        let x = *target_pos % 4;
                        let y = *target_pos / 4;

                        let candidates = [(x, y), (x + 1, y), (x, y + 1), (x + 1, y + 1)];

                        for (nx, ny) in candidates {
                            if nx < 4 && ny < 4 {
                                let index = ny * 4 + nx;
                                if self.game_state.active_aliens[index] > 0 {
                                    self.game_state.active_aliens[index] += 1;
                                }
                            }
                        }
                    }
                }

                calculate_king_alien_lvl(self);
                self.daily.daily_powerups.0 += 1;
                if (self.daily.daily_powerups.0 == self.daily.daily_powerups.1) {
                    self.daily.daily_powerups.2 = true;
                    self.daily.total_completed += 1;
                    self.progress.social_score += 2;
                }

                Response::ok(
                    json!({
                        "active_aliens": self.game_state.active_aliens,
                        "power_ups": self.game_state.power_ups,
                        "king_lvl": self.game_state.king_lvl,
                        "product" : self.progress.product,
                        "links": self.daily.links,
                        "daily_merge": self.daily.daily_merge,
                        "daily_annotate": self.daily.daily_annotate,
                        "daily_powerups": self.daily.daily_powerups,
                        "total_completed": self.daily.total_completed,
                        "alien_earned": self.daily.alien_earned,
                        "pu_earned": self.daily.pu_earned
                    })
                    .to_string(),
                )
            }
            Op::AwardBadge(badge) => {
                self.progress.badges.push(badge.clone());
                Response::ok(
                    json!({
                        "badges": self.progress.badges
                    })
                    .to_string(),
                )
            }
            Op::GetData => {
                let current_time = Date::now().as_millis() / 1000;
                let time_since_last_login = current_time - self.profile.real_login;
                let one_hour = 60 * 60;


                if time_since_last_login >= one_hour {
                    self.game_state.inventory_aliens += 20;
                    self.profile.real_login = Date::now().as_millis() / 1000;
                }
            
                
                Response::from_json(&self)
            }
            Op::Register(password) => {
                console_log!("Creating tables if not exists");
                let sha256 = sha2::Sha256::new();
                let password = sha256.chain(password.as_bytes()).finalize();
                let password = hex::encode(password);

                self.profile.password = Some(password.clone());

                match insert_new_user(&self, &d1).await {
                    Ok(_) => Response::ok("User registered successfully!"),
                    Err(e) => {
                        console_error!("Registration failed: {:?}", e);
                        Response::error("Registration failed", 500)
                    }
                }
            }

            // Profile operations
            Op::UpdateEmail(email) => {
                self.profile.email = Some(email.clone());
                Response::ok(
                    json!({
                        "email": self.profile.email
                    })
                    .to_string(),
                )
            }
            Op::UpdatePfp(pfp) => {
                self.profile.pfp = pfp.clone();
                Response::ok(
                    json!({
                        "pfp": self.profile.pfp
                    })
                    .to_string(),
                )
            }

            // Progress operations
            Op::UpdateIq(iq) => {
                self.progress.iq = *iq;
                calculate_product(self);
                Response::ok(
                    json!({
                        "iq": self.progress.iq,
                        "product" : self.progress.product
                    })
                    .to_string(),
                )
            }

            Op::IncrementAkaiBalance => {
                self.progress.akai_balance += 1;
                Response::ok(
                    json!({
                        "akai_balance": self.progress.akai_balance
                    })
                    .to_string(),
                )
            }
            Op::DecrementAkaiBalance => {
                if self.progress.akai_balance > 0 {
                    self.progress.akai_balance -= 1;
                }
                Response::ok(
                    json!({
                        "akai_balance": self.progress.akai_balance
                    })
                    .to_string(),
                )
            }

            Op::DeleteAlienFromActive(idx) => {
                self.game_state.active_aliens[*idx] = 0;
                calculate_king_alien_lvl(self);
                Response::ok(
                    json!({
                        "active_aliens": self.game_state.active_aliens,
                        "king_lvl" : self.game_state.king_lvl,
                    })
                    .to_string(),
                )
            }
            Op::UpdateUserName(user_name) => {
                self.profile.user_name = user_name.clone();

                Response::ok(
                    json!({
                        "user_name": self.profile.user_name
                    })
                    .to_string(),
                )
            }
            Op::UpdatePassword(password) => {
                let sha256 = sha2::Sha256::new();
                let password = sha256.chain(password.as_bytes()).finalize();
                let password = hex::encode(password);

                self.profile.password = Some(password.clone());

                Response::ok(
                    json!({
                        "password": self.profile.password
                    })
                    .to_string(),
                )
            }
            Op::MoveAlienInGrid(from, to) => {
                if *from >= 16 || *to >= 16 {
                    return Response::error("Invalid grid position", 400);
                }

                self.game_state.active_aliens.swap(*from, *to);

                Response::ok(
                    json!({
                        "active_aliens": self.game_state.active_aliens
                    })
                    .to_string(),
                )
            }
            Op::AddNotificationInternal(notification) => {
                if notification.notification_type == NotificationType::Referral {
                    self.social.players_referred += 1;
                    self.progress.social_score += 10;
                    self.progress.akai_balance += 50;
                } else if notification.notification_type == NotificationType::Performance {
                    if let Some(metadata) = &notification.metadata {
                        if let Some(akai_str) = metadata.get("akai_balance") {
                            if let Ok(akai) = akai_str.parse::<usize>() {
                                self.progress.akai_balance += akai;
                            }
                        }

                        if let Some(iq_str) = metadata.get("iq") {
                            if let Ok(iq_change) = iq_str.parse::<isize>() {
                                let current_iq = self.progress.iq as isize;
                                let new_iq = (current_iq + iq_change).max(0);
                                self.progress.iq = new_iq as usize;
                                
                            }
                        }
                    }
                }
                self.notifications.push(notification.clone());
                calculate_product(self);
                Response::ok(
                    json!({
                        "status": "Notification added to DO",
                        "players_referred": self.social.players_referred
                    })
                    .to_string(),
                )
            }
            
            Op::MarkNotificationRead(notification_id) => {
                let mut found = false;

                for notif in self.notifications.iter_mut() {
                    if notif.notification_id == *notification_id {
                        notif.read = Read::Yes;
                        found = true;
                        break;
                    }
                }

                if found {
                    Response::ok(
                        json!({
                            "status": "marked as read",
                            "notification_id": notification_id,
                            "notifications": self.notifications
                        })
                        .to_string(),
                    )
                } else {
                    Response::error("Notification not found", 404)
                }
            }
            
            Op::UseReferralCode(code) => {
                let env = env.clone();

                match find_user_id_by_referral_code(&d1, code).await {
                    Ok(Some(referrer_user_id)) => {
                        let message = "Your referral code was used!";
                        let mut metadata = HashMap::new();
                        metadata.insert("used_by".to_string(), op_request.user_id.clone());
                        metadata.insert("social_score".to_string(), "10".to_string());
                        metadata.insert("akai_balance".to_string(), "25".to_string());
                        if let Err(e) = push_notification_to_user_do(
                            &env,
                            &referrer_user_id,
                            NotificationType::Referral,
                            message,
                            Some(metadata),
                        )
                        .await
                        {
                            console_error!("Failed to push referral notification: {:?}", e);
                            return Response::error("Internal error", 500);
                        }
                        Response::ok(
                            json!({
                                "status": "Referral recorded",
                                "referrer": referrer_user_id
                            })
                            .to_string(),
                        )
                    }
                    Ok(None) => Response::error("Invalid referral code", 404),
                    Err(e) => {
                        console_error!("DB error during referral lookup: {:?}", e);
                        Response::error("Database error", 500)
                    }
                }
            }
            
            Op::UpdateDbFromDo => match crate::sql::update_user_data(self, d1).await {
                Ok(_) => Response::ok(
                    json!({
                        "status": "Database successfully updated from DO"
                    })
                    .to_string(),
                ),
                Err(e) => {
                    console_error!("Error updating DB from DO: {:?}", e);
                    Response::error("Failed to update DB", 500)
                }
            },
            
            Op::GenerateDailyTasks => {
                let now = worker::Date::now().as_millis();
                let one_day_in_millis = 24 * 60 * 60 * 1000;

                if now - self.daily.last_task_generation < one_day_in_millis {
                    return Response::error(
                        json!({"error": "A new set of tasks is not available yet."}).to_string(),
                        429, 
                    );
                }
                
                self.daily.last_task_generation = now;
                
                console_log!("100");

                let mut rng = rand::thread_rng();
                let random_links = get_random_links(2)
                    .into_iter()
                    .map(|sl| Links {
                        url: sl.url,
                        platform: sl.platform,
                        visited: false,
                    })
                    .collect();

                let level = self.progress.iq / 50 + 1;
                let num_mcq_tasks = level * 4;
                let num_text_tasks = level * 6;

                let mcq_tasks = fetch_mcq_video_tasks(num_mcq_tasks, &env)
                    .await
                    .unwrap_or_default();

                let text_tasks = fetch_text_video_tasks(num_text_tasks, &env)
                    .await
                    .unwrap_or_default();

                console_log!("MCQ Tasks fetched: {}", mcq_tasks.len());
                console_log!("Text Tasks fetched: {}", text_tasks.len());

                self.daily.links = random_links;
                self.daily.mcq_video_tasks = mcq_tasks;
                self.daily.text_video_tasks = text_tasks;
                self.daily.daily_merge = (0, rng.gen_range(2..=4), false);
                self.daily.daily_annotate = (0, rng.gen_range(3..=7), false);
                self.daily.daily_powerups = (0, rng.gen_range(2..=6), false);
                self.daily.alien_earned = None;
                self.daily.pu_earned = None;
                self.daily.total_completed = 0;

                Response::from_json(&self.daily)
            }

            Op::SubmitMcqAnswers(datapoint_id, answers) => {
                if answers.len() != 5 {
                    return Response::error(json!({"error": "Exactly 5 answers are required."}).to_string(), 400);
                }

                let task_index = match self
                    .daily
                    .mcq_video_tasks
                    .iter()
                    .position(|task| task.id == datapoint_id.clone())
                {
                    Some(index) => index,
                    None => return Response::error(json!({"error": "Task not found"}).to_string(), 404),
                };

                if self.daily.mcq_video_tasks[task_index].visited {
                    return Response::error(json!({"error": "Task already completed"}).to_string(), 400);
                }

                let questions = &self.daily.mcq_video_tasks[task_index].preLabel.questions;
                if questions.len() != 5 {
                    return Response::error(json!({"error": "Task data is invalid."}).to_string(), 500);
                }

                // Map answers to questions for the backend payload
                let answer_obj: HashMap<String, String> = answers
                    .iter()
                    .enumerate()
                    .map(|(index, answer)| (index.to_string(), answer.clone()))
                    .collect();

                let payload = json!({
                    "datapointId": datapoint_id,
                    "playerId": op_request.user_id,
                    "answerObj": answer_obj
                });

                // Submit to external endpoint
                let req = Request::new_with_init(
                    &format!("{}/api/game/add-mcq-answer", BASE_URL), // Replace YOUR_LOCAL_IP
                    &RequestInit {
                        method: Method::Post,
                        body: Some(payload.to_string().into()),
                        headers: {
                            let mut headers = Headers::new();
                            headers.set("Content-Type", "application/json")?;
                            headers
                        },
                        ..Default::default()
                    },
                )?;

                let res = Fetch::Request(req).send().await?;
                if !res.status_code() == 200 {
                    return Response::error(json!({"error": "Failed to submit answers to backend"}).to_string(), 500);
                }

                // Update local state after successful submission
                self.daily.mcq_video_tasks[task_index].visited = true;
                self.daily.daily_annotate.0 += 1;
                if self.daily.daily_annotate.0 == self.daily.daily_annotate.1 {
                    self.daily.daily_annotate.2 = true;
                    self.daily.total_completed += 1;
                    self.progress.social_score += 2;
                }

                calculate_product(self);

                Response::ok(
                    json!({
                        "status": "MCQ answers submitted successfully",
                        "daily": &self.daily
                    })
                    .to_string(),
                )
            }

            Op::SubmitTextAnswer(datapoint_id, idx, text) => {
                let task_index = match self
                    .daily
                    .text_video_tasks
                    .iter()
                    .position(|task| task.datapointId == datapoint_id.clone() && task.questionIndex == idx.clone())
                {
                    Some(index) => index,
                    None => return Response::error(json!({"error": "Task not found"}).to_string(), 404),
                };

                if self.daily.text_video_tasks[task_index].visited {
                    return Response::error(json!({"error": "Task already completed"}).to_string(), 400);
                }

                // Submit to external endpoint first
                let payload = json!({
                    "datapointId": datapoint_id,
                    "playerId": op_request.user_id,
                    "idx": idx,
                    "text": text,
                });

                let req = Request::new_with_init(
                    &format!("{}/api/game/add-text-answer", BASE_URL), // Replace YOUR_LOCAL_IP
                    &RequestInit {
                        method: Method::Post,
                        body: Some(payload.to_string().into()),
                        headers: {
                            let mut headers = Headers::new();
                            headers.set("Content-Type", "application/json")?;
                            headers
                        },
                        ..Default::default()
                    },
                )?;

                let res = Fetch::Request(req).send().await?;
                if !res.status_code() == 200 {
                    return Response::error(json!({"error": "Failed to submit text answer to backend"}).to_string(), 500);
                }

                // Update local state after successful submission
                self.daily.text_video_tasks[task_index].visited = true;
                self.daily.daily_annotate.0 += 1;
                if self.daily.daily_annotate.0 == self.daily.daily_annotate.1 {
                    self.daily.daily_annotate.2 = true;
                    self.daily.total_completed += 1;
                    self.progress.social_score += 2;
                }

                calculate_product(self);

                Response::ok(
                    json!({
                        "status": "Text answer submitted successfully",
                        "daily": &self.daily
                    })
                    .to_string(),
                )
            }

            Op::CheckDailyTask(maybe_url) => {
                let mut matched = false;

                if let Some(url) = maybe_url {
                    for link in &mut self.daily.links {
                        if link.url == *url && !link.visited {
                            link.visited = true;
                            self.progress.social_score += 2;
                            matched = true;
                            break; // Exit loop once matched and updated
                        }
                    }

                    if matched {
                        self.daily.total_completed += 1;
                    }
                }
                Response::from_json(&self.daily)
            }

            Op::ClaimDailyReward(index) => {
                give_daily_reward(self, *index);
                Response::ok(
                    json!({
                        "active_aliens": self.game_state.active_aliens,
                        "king_lvl": self.game_state.king_lvl,
                        "product": self.progress.product,
                        "alien_earned": self.daily.alien_earned,
                        "pu_earned": self.daily.pu_earned,
                        "power_ups": self.game_state.power_ups
                    })
                    .to_string(),
                )
            }
            
            Op::SyncData => match crate::sql::update_user_data(self, d1).await {
                Ok(_) => Response::ok("Data synced successfully"),
                Err(e) => {
                    console_error!("Error syncing data: {:?}", e);
                    Response::error("Failed to sync data", 500)
                }
            }
            
        }
    }
}
