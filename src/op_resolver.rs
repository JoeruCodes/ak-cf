use crate::daily_task::Links;
use crate::gpt_voice::*;
use crate::notification::{push_notification_to_user_do, NotificationType};
use crate::types::DurableObjectAugmentedMsg;
use crate::utils::find_user_id_by_referral_code;
use crate::{daily_task::*, gpt_voice};
use rand::Rng;
use serde_json::json;
use sha2::digest::Update;
use sha2::Digest;
use worker::{console_error, console_log, D1Database, Env, Response, Result};

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
                self.game_state.active_aliens[*idx_b] =
                    if !self.game_state.inventory_aliens.is_empty() {
                        self.game_state.inventory_aliens.pop().unwrap()
                    } else {
                        0
                    };
                self.game_state.total_merged_aliens += 1;
                self.daily.daily_merge.0 += 1;
                calculate_king_alien_lvl(self);
                Response::ok(
                    json!({
                        "active_aliens": self.game_state.active_aliens,
                        "inventory_aliens": self.game_state.inventory_aliens,
                        "total_merged_aliens": self.game_state.total_merged_aliens,
                        "king_lvl": self.game_state.king_lvl,
                        "product" : self.progress.product,
                        "daily_merge" : self.daily.daily_merge
                    })
                    .to_string(),
                )
            }
            Op::SpawnAlien => {
                let alien_lvl = self
                    .game_state
                    .active_aliens
                    .iter()
                    .max()
                    .unwrap_or(&5)
                    .max(&5)
                    - 4;
                if self.game_state.active_aliens.iter().all(|a| *a != 0) {
                    self.game_state.inventory_aliens.push(alien_lvl);
                } else {
                    for i in 0..self.game_state.active_aliens.len() {
                        if self.game_state.active_aliens[i] == 0 {
                            self.game_state.active_aliens[i] = alien_lvl;
                            break;
                        }
                    }
                    calculate_king_alien_lvl(self);
                }
                Response::ok(
                    json!({
                        "active_aliens": self.game_state.active_aliens,
                        "inventory_aliens": self.game_state.inventory_aliens,
                        "total_merged_aliens": self.game_state.total_merged_aliens,
                        "king_lvl": self.game_state.king_lvl,
                        "product" : self.progress.product
                    })
                    .to_string(),
                )
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

                Response::ok(
                    json!({
                        "active_aliens": self.game_state.active_aliens,
                        "power_ups": self.game_state.power_ups,
                        "king_lvl": self.game_state.king_lvl,
                        "product" : self.progress.product,
                        "daily_powerups" : self.daily.daily_powerups,
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
            Op::GetData => Response::from_json(&self),
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
            // Op::UpdateLastLogin(time) => {
            //     self.profile.last_login = *time;
            //     Response::ok(
            //         json!({
            //             "last_login": self.profile.last_login
            //         })
            //         .to_string(),
            //     )
            // }

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
            Op::UpdateSocialScore(score) => {
                self.progress.social_score = *score;
                calculate_product(self);
                Response::ok(
                    json!({
                        "social_score": self.progress.social_score,
                        "product" : self.progress.product
                    })
                    .to_string(),
                )
            }
            Op::MoveAlienFromInventoryToActive => {
                match self.game_state.active_aliens.iter().position(|a| *a == 0) {
                    Some(alien) => {
                        let inven = self.game_state.inventory_aliens.pop();

                        match inven {
                            Some(inven) => {
                                self.game_state.active_aliens[alien] = inven;

                                Response::ok(
                                    json!({
                                        "active_aliens": self.game_state.active_aliens,
                                        "inventory_aliens": self.game_state.inventory_aliens
                                    })
                                    .to_string(),
                                )
                            }
                            None => Response::error("No aliens in inventory", 404),
                        }
                    }
                    None => Response::error("Active aliens full!", 404),
                }
            }
            // Op::UpdateAllTaskDone(done) => {
            //     self.progress.all_task_done = *done;
            //     Response::ok(
            //         json!({
            //             "all_task_done": self.progress.all_task_done
            //         })
            //         .to_string(),
            //     )
            // }
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
            // Op::IncrementTotalTaskCompleted => {
            //     self.progress.total_task_completed += 1;
            //     Response::ok(
            //         json!({
            //             "total_task_completed": self.progress.total_task_completed
            //         })
            //         .to_string(),
            //     )
            // }

            // Social operations
            // Op::IncrementPlayersReferred => {
            //     self.social.players_referred += 1;
            //     Response::ok(
            //         json!({
            //             "players_referred": self.social.players_referred
            //         })
            //         .to_string(),
            //     )
            // }

            // League operations
            Op::UpdateLeague(league) => {
                self.league = league.clone();
                Response::ok(
                    json!({
                        "league": self.league
                    })
                    .to_string(),
                )
            }
            // Op::DeleteAlienFromInventory(idx) => {
            //     self.game_state.inventory_aliens.remove(*idx);
            //     Response::ok(
            //         json!({
            //             "inventory_aliens": self.game_state.inventory_aliens
            //         })
            //         .to_string(),
            //     )
            // }
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
                        "user_name": self.profile.password
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
                // Fetch env separately in the DO and pass it into this method
                let env = env.clone(); // Make sure you pass it into resolve_op beforehand

                match find_user_id_by_referral_code(&d1, code).await {
                    Ok(Some(referrer_user_id)) => {
                        let message = "Your referral code was used!";
                        if let Err(e) = push_notification_to_user_do(
                            &env, // ✅ pass the env from the outside
                            &referrer_user_id,
                            NotificationType::Referral,
                            message,
                            None,
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
            // Op::UpdateDbFromDo => match crate::sql::update_user_data(self, d1).await {
            //     Ok(_) => Response::ok(
            //         json!({
            //             "status": "Database successfully updated from DO"
            //         })
            //         .to_string(),
            //     ),
            //     Err(e) => {
            //         console_error!("Error updating DB from DO: {:?}", e);
            //         Response::error("Failed to update DB", 500)
            //     }
            // },
            Op::GenerateDailyTasks => {
                let now = worker::Date::now().as_millis() as u64 / 1000;
                let q_seconds = 30; // 1 day interval

                let last_login = self.profile.last_login;

                if now - last_login < q_seconds && !self.daily.links.is_empty() {
                    return Response::from_json(&self.daily);
                }

                let mut rng = rand::thread_rng();
                let random_links = get_random_links(2)
                    .into_iter()
                    .map(|sl| Links {
                        url: sl.url,
                        platform: sl.platform,
                        visited: false,
                    })
                    .collect();

                self.daily.links = random_links;
                self.daily.daily_merge = (0, rng.gen_range(15..=26), false);
                self.daily.daily_annotate = (0, rng.gen_range(3..=7), false);
                self.daily.daily_powerups = (0, rng.gen_range(2..=6), false);
                self.profile.last_login = now;

                Response::from_json(&self.daily)
            }
            Op::CheckDailyTask(maybe_url) => {
                // Check if a link was passed and mark it as visited if matched
                if let Some(url) = maybe_url {
                    for link in &mut self.daily.links {
                        if link.url == *url {
                            link.visited = true;
                        }
                    }
                }

                let check_and_mark = |(current, target, _): (usize, usize, bool)| {
                    let is_complete = current >= target;
                    (current, target, is_complete)
                };

                self.daily.daily_merge = check_and_mark(self.daily.daily_merge);
                self.daily.daily_annotate = check_and_mark(self.daily.daily_annotate);
                self.daily.daily_powerups = check_and_mark(self.daily.daily_powerups);

                Response::from_json(&self.daily)
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
        }
    }
}
