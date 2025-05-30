use crate::daily_task::Links;
use crate::gpt_voice::*;
use crate::notification::{push_notification_to_user_do, NotificationType};
use crate::types::DurableObjectAugmentedMsg;
use crate::utils::{fetch_video_tasks, find_user_id_by_referral_code, give_daily_reward};
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
                self.profile.last_login = Date::now().as_millis() / 1000;
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
                self.profile.password = password.clone();

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
                    self.progress.akai_balance += 25;
                } else if notification.notification_type == NotificationType::Performance {
                    if let Some(metadata) = &notification.metadata {
                        if let Some(akai_str) = metadata.get("akai_earned") {
                            if let Ok(akai) = akai_str.parse::<usize>() {
                                self.progress.akai_balance += akai;
                            }
                        }

                        if let Some(iq_str) = metadata.get("iq_change") {
                            if let Ok(iq) = iq_str.parse::<usize>() {
                                self.progress.iq += iq;
                            }
                        }
                    }
                }

                self.notifications.push(notification.clone());

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
                console_log!("100");
                let now = worker::Date::now().as_millis() as u64 / 1000;
                let q_seconds = 1000; // 1 day interval

                let last_login = self.profile.last_login;

                // if now - last_login < q_seconds && !self.daily.links.is_empty() {
                //     return Response::from_json(&self.daily);
                // }

                // if self.daily.total_completed < 3 && self.daily.links.len() > 0 {
                //     self.progress.social_score -= 5;
                // }

                let mut rng = rand::thread_rng();
                let random_links = get_random_links(2)
                    .into_iter()
                    .map(|sl| Links {
                        url: sl.url,
                        platform: sl.platform,
                        visited: false,
                    })
                    .collect();

                let number_of_videos_to_request = ((self.progress.iq) / 50 + 1) * 5;

                let video_tasks = fetch_video_tasks(number_of_videos_to_request, &env)
                    .await
                    .unwrap_or_default();

                console_log!("{}",video_tasks.len());

                self.daily.links = random_links;
                self.daily.video_tasks = video_tasks;
                self.daily.daily_merge = (0, rng.gen_range(2..=4), false);
                self.daily.daily_annotate = (0, rng.gen_range(3..=7), false);
                self.daily.daily_powerups = (0, rng.gen_range(2..=6), false);
                self.profile.last_login = now;
                self.daily.alien_earned = None;
                self.daily.pu_earned = None;
                self.daily.total_completed = 0;

                Response::from_json(&self.daily)
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
            },
            Op::RequestVoiceKey => match gpt_voice::fetch_gpt_voice_key(env).await {
                Ok(secret) => Response::ok(secret),
                Err(e) => {
                    console_log!("Error fetching GPT Voice token: {:?}", e);
                    Response::error("Failed to fetch voice token", 500)
                }
            },
            Op::SubmitVideoLabel(datapoint_id, label) => {
                let payload = serde_json::json!({
                    "datapointId": datapoint_id,
                    "label": label
                })
                .to_string();

                let req = Request::new_with_init(
                    "http://localhost:3001/api/game/label-datapoint",
                    &RequestInit {
                        method: Method::Post,
                        body: Some(payload.into()),
                        headers: {
                            let mut headers = Headers::new();
                            headers.set("Content-Type", "application/json")?;
                            headers
                        },
                        ..Default::default()
                    },
                )?;

                let res = Fetch::Request(req).send().await?;
                let status = res.status_code();

                if status >= 200 && status < 300 {
                    Response::from_json(&serde_json::json!({
                        "message": "Label submitted successfully"
                    }))
                } else {
                    Response::from_json(&serde_json::json!({
                        "error": "Failed to submit label",
                        "status": status
                    }))
                }
            }
        }
    }
}
