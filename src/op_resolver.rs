use serde_json::json;
use worker::{console_error, console_log, D1Database, Response, Result};

use crate::{
    sql::insert_new_user,
    types::{Op, PowerUpKind, UserData, WsMsg},
    utils::calculate_king_alien_lvl,
    utils::calculate_product,
};

impl UserData {
    pub async fn resolve_op(&mut self, op_request: &WsMsg, d1: &D1Database) -> Result<Response> {
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
                calculate_king_alien_lvl(self);
                Response::ok(
                    json!({
                        "active_aliens": self.game_state.active_aliens,
                        "inventory_aliens": self.game_state.inventory_aliens,
                        "total_merged_aliens": self.game_state.total_merged_aliens,
                        "king_lvl": self.game_state.king_lvl
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
                        "king_lvl": self.game_state.king_lvl
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
            Op::UsePowerup(idx) => {
                let power_up = self.game_state.power_ups.swap_remove(*idx);

                 match power_up {
                    PowerUpKind::ColumnPowerUp => {
                        for i in 0..4 {
                            self.game_state.active_aliens[i] += 1;
                        }
                    }
                    PowerUpKind::RowPowerUp => {
                        for i in 0..4 {
                            self.game_state.active_aliens[i * 4] += 1;
                        }
                    }
                    PowerUpKind::NearestSquarePowerUp => {
                        for i in 0..4 {
                            self.game_state.active_aliens[i * 4] += 1;
                            self.game_state.active_aliens[i * 4 + 1] += 1;
                        }
                    }
                }
                Response::ok(
                    json!({
                        "power_ups": self.game_state.power_ups
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
            Op::Register => {
                console_log!("Creating tables if not exists");
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
            Op::UpdateLastLogin(time) => {
                self.profile.last_login = *time;
                Response::ok(
                    json!({
                        "last_login": self.profile.last_login
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
                        "iq": self.progress.iq
                    })
                    .to_string(),
                )
            }
            Op::UpdateSocialScore(score) => {
                self.progress.social_score = *score;
                calculate_product(self);
                Response::ok(
                    json!({
                        "social_score": self.progress.social_score
                    })
                    .to_string(),
                )
            }
            Op::MoveAlienFromInventoryToActive => {
                match self.game_state.active_aliens.iter().position(|a| *a == 0){
                    Some(alien) => {
                        let inven = self.game_state.inventory_aliens.pop();
                        
                        match inven{
                            Some(inven) =>  {
                                self.game_state.active_aliens[alien] = inven;

                                Response::ok(json!({
                                    "active_aliens": self.game_state.active_aliens,
                                    "inventory_aliens": self.game_state.inventory_aliens
                                }).to_string())
                            }
                            None => Response::error("No aliens in inventory", 404)
                        }
                    },
                    None => Response::error("Active aliens full!", 404)
                }
            }
            Op::UpdateAllTaskDone(done) => {
                self.progress.all_task_done = *done;
                Response::ok(
                    json!({
                        "all_task_done": self.progress.all_task_done
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
            Op::IncrementTotalTaskCompleted => {
                self.progress.total_task_completed += 1;
                Response::ok(
                    json!({
                        "total_task_completed": self.progress.total_task_completed
                    })
                    .to_string(),
                )
            }

            // Social operations
            Op::IncrementPlayersReferred => {
                self.social.players_referred += 1;
                Response::ok(
                    json!({
                        "players_referred": self.social.players_referred
                    })
                    .to_string(),
                )
            }

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
            Op::DeleteAlienFromInventory(idx) => {
                self.game_state.inventory_aliens.remove(*idx);
                Response::ok(
                    json!({
                        "inventory_aliens": self.game_state.inventory_aliens
                    })
                    .to_string(),
                )
            }
            Op::DeleteAlienFromActive(idx) => {
                self.game_state.active_aliens[*idx] = 0;
                Response::ok(
                    json!({
                        "active_aliens": self.game_state.active_aliens
                    })
                    .to_string(),
                )
            }
            Op::UpdateUserName(user_name) => {
                self.profile.user_name = user_name.clone();

                Response::ok(json!({
                    "user_name": self.profile.user_name
                }).to_string())
            }
            Op::UpdatePassword(password) => {
                self.profile.password = password.clone();

                Response::ok(json!({
                    "user_name": self.profile.password
                }).to_string())
            }
        }
    }
}
