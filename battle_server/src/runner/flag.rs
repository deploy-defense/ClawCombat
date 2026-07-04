use battle_core::{
    behavior::{Behavior, Body},
    game::{
        flag::{FlagOwnership, FlagsOwnership},
        Side,
    },
    order::Order,
    physics::utils::distance_between_points,
    state::battle::message::{BattleStateMessage, SoldierMessage},
};

use super::{message::RunnerMessage, Runner};

impl Runner {
    pub fn tick_flags(&self) -> Vec<RunnerMessage> {
        puffin::profile_scope!("tick_flags");
        if self.battle_state.frame_i() % self.config.flags_update_freq() == 0 {
            let mut new_ownerships = vec![];
            let mut messages = vec![];
            
            let current_frame = *self.battle_state.frame_i();
            let mut flag_capture_data = self.flag_capture_data.write().unwrap();
            let mut flag_cooldown_data = self.flag_cooldown_data.write().unwrap();

            // 만료된 쿨다운 삭제 (예: 60초 = 3600프레임 후 쿨다운 해제)
            flag_cooldown_data.retain(|_, (expire_frame, _)| *expire_frame > current_frame);

            for (flag_name, ownership) in self.battle_state.flags().ownerships() {
                let flag = self.battle_state.map().flag(flag_name);
                
                // 분대별 점령 식별 및 쿨다운 필터링
                let mut a_squads_inside = vec![];
                let mut b_squads_inside = vec![];
                for s in self.battle_state.soldiers() {
                    if s.can_take_flag() && flag.shape().contains(&s.world_point()) {
                        if s.side() == &Side::A {
                            a_squads_inside.push(s.squad_uuid());
                        } else if s.side() == &Side::B {
                            b_squads_inside.push(s.squad_uuid());
                        }
                    }
                }

                // 쿨다운에 걸린 분대는 점령에 기여할 수 없음 (무시)
                a_squads_inside.retain(|sq| {
                    if let Some((expire_frame, cooled_sq)) = flag_cooldown_data.get(&flag_name.0) {
                        if *expire_frame > current_frame && sq == cooled_sq {
                            false // 아직 쿨다운 중인 분대는 제외
                        } else {
                            true // 쿨다운이 만료되었거나 다른 분대는 통과
                        }
                    } else {
                        true
                    }
                });
                b_squads_inside.retain(|sq| {
                    if let Some((expire_frame, cooled_sq)) = flag_cooldown_data.get(&flag_name.0) {
                        if *expire_frame > current_frame && sq == cooled_sq {
                            false // 아직 쿨다운 중인 분대는 제외
                        } else {
                            true // 쿨다운이 만료되었거나 다른 분대는 통과
                        }
                    } else {
                        true
                    }
                });

                let a_inside = !a_squads_inside.is_empty();
                let b_inside = !b_squads_inside.is_empty();

                let mut new_ownership = match (ownership, a_inside, b_inside) {
                    (FlagOwnership::Nobody, true, true) => FlagOwnership::Both,
                    (FlagOwnership::Nobody, true, false) => FlagOwnership::A,
                    (FlagOwnership::Nobody, false, true) => FlagOwnership::B,
                    (FlagOwnership::Nobody, false, false) => FlagOwnership::Nobody,
                    (FlagOwnership::A, true, true) => FlagOwnership::Both,
                    (FlagOwnership::A, true, false) => FlagOwnership::A,
                    (FlagOwnership::A, false, true) => FlagOwnership::B,
                    (FlagOwnership::A, false, false) => FlagOwnership::A,
                    (FlagOwnership::B, true, true) => FlagOwnership::Both,
                    (FlagOwnership::B, true, false) => FlagOwnership::A,
                    (FlagOwnership::B, false, true) => FlagOwnership::B,
                    (FlagOwnership::B, false, false) => FlagOwnership::B,
                    (FlagOwnership::Both, true, true) => FlagOwnership::Both,
                    (FlagOwnership::Both, true, false) => FlagOwnership::A,
                    (FlagOwnership::Both, false, true) => FlagOwnership::B,
                    (FlagOwnership::Both, false, false) => FlagOwnership::Both,
                };

                // 점령 시간 추적 및 자동 해제 로직
                if new_ownership == FlagOwnership::A || new_ownership == FlagOwnership::B {
                    let capturing_squad = if new_ownership == FlagOwnership::A {
                        a_squads_inside.first().copied()
                    } else {
                        b_squads_inside.first().copied()
                    };

                    // [수정] 깃발 내부에 병사가 없더라도, 한 번 점령된 깃발은 타이머가 계속 돌아가서 일정 시간 뒤에 자연스럽게 해제되게 합니다.
                    let fallback_sq = battle_core::types::SquadUuid(0);
                    let active_sq = capturing_squad.unwrap_or(fallback_sq);

                    let entry = flag_capture_data.entry(flag_name.0.clone()).or_insert((current_frame, active_sq));
                    
                    // 만약 새로운 병사(분대)가 들어왔고, 기존 점령 분대와 다르면 타이머 리셋
                    if capturing_squad.is_some() && entry.1 != active_sq && entry.1 != fallback_sq {
                        *entry = (current_frame, active_sq);
                    } else if entry.1 == fallback_sq && capturing_squad.is_some() {
                        *entry = (current_frame, active_sq);
                    }

                    let duration = current_frame.saturating_sub(entry.0);
                    if duration >= 1800 { // 30초 초과 시 자동 해제
                        let sq = entry.1;
                        // 구조체에 존재하지 않는 name() 메서드 대신, 고유 UUID 인덱스 번호를 조합하여 컴파일 에러가 없는 명확한 분대명 문자열 생성
                        let squad_name_str = format!("Squad_{}", sq.0);
                        
                        println!("[거점 순환 자연 해제] 거점 [{}]의 소유권이 30초 점유 시간 경과로 자연스럽게 중립화됩니다. (해제된 분대명: {})", flag_name.0, squad_name_str);
                        new_ownership = FlagOwnership::Nobody;
                        
                        // [주홍글씨 로직 반영] 동일한 분대가 다시는 이 flag를 점령할 수 없도록 만료 제한 시간을 u64::MAX로 설정하여 영구 차단
                        flag_cooldown_data.insert(flag_name.0.clone(), (u64::MAX, sq));
                        flag_capture_data.remove(&flag_name.0);

                        // 무조건 해당 공간을 나가 다른 해제되거나 점령 가능한 flag를 찾아 기동하도록 처리
                        if sq != fallback_sq {
                            if let Some(squad) = self.battle_state.squads().get(&sq) {
                                let leader_idx = squad.leader();
                                let leader = self.battle_state.soldier(leader_idx);
                                let is_side_a = leader.side() == &Side::A;
                                let is_side_b = leader.side() == &Side::B;
                                
                                // 맵 상의 깃발 중 해당 분대가 새로 기동해 들어갈 수 있는(주홍글씨 영구 제한에 걸리지 않고 아군 소유가 아닌) 타겟 깃발 탐색
                                let map = self.battle_state.map();
                                let mut next_target_flag = None;
                                let mut min_distance = std::f32::MAX;
                                
                                for alternative_flag in map.flags() {
                                    // 현재 해제된 이 거점 자체는 주홍글씨 데이터에 u64::MAX로 방금 주입되었으므로 검사 과정에서 자연스럽게 제외됨
                                    let is_alternative_cooldown = if let Some((_, cooled_sq)) = flag_cooldown_data.get(&alternative_flag.name().0) {
                                        *cooled_sq == sq
                                    } else {
                                        false
                                    };
                                    if is_alternative_cooldown {
                                        continue;
                                    }
                                    
                                    // 이미 아군 진영이 점령 완료한 거점인지 여부 검사
                                    let is_already_owned = self.battle_state.flags().ownerships().iter().any(|(n, o)| {
                                        n == alternative_flag.name() && (
                                            o == &FlagOwnership::Both || 
                                            (is_side_a && o == &FlagOwnership::A) ||
                                            (is_side_b && o == &FlagOwnership::B)
                                        )
                                    });
                                    
                                    if !is_already_owned {
                                        let dist = battle_core::physics::utils::distance_between_points(&leader.world_point(), &alternative_flag.position()).meters() as f32;
                                        if dist < min_distance {
                                            min_distance = dist;
                                            next_target_flag = Some(alternative_flag.clone());
                                        }
                                    }
                                }
                                
                                // 새로 진격할 대상 거점을 성공적으로 식별한 경우 A* 경로를 주입하여 즉시 출격
                                if let Some(target_flag_found) = next_target_flag {
                                    let from_grid = map.grid_point_from_world_point(&leader.world_point());
                                    let to_grid = map.grid_point_from_world_point(&target_flag_found.position());
                                    
                                    if from_grid != to_grid {
                                        if let Some(grid_path) = battle_core::physics::path::find_path(
                                            &self.config,
                                            map,
                                            &from_grid,
                                            &to_grid,
                                            true,
                                            &battle_core::physics::path::PathMode::Walk,
                                            &Some(battle_core::physics::path::Direction::from_angle(&leader.get_looking_direction())),
                                        ) {
                                            let world_path = grid_path.iter().map(|p| map.world_point_from_grid_point(*p)).collect();
                                            let world_paths = battle_core::types::WorldPaths::new(vec![battle_core::types::WorldPath::new(world_path)]);
                                            
                                            // 거점 탈환을 위한 신속 기동 명령 생성 및 최종 도달 시 방어 태세 전환 예약 설정
                                            let final_move_order = Order::MoveFastTo(world_paths.clone(), Some(Box::new(Order::Defend(battle_core::types::Angle(0.0)))));
                                            
                                            println!("[거점 이탈 및 진격] 분대 {}가 해제된 구역을 탈출하여 새로운 거점 [{}] 확보를 위해 전속력으로 이동합니다.", squad_name_str, target_flag_found.name().0);
                                            
                                            // 분대가 포복(crawling) 상태에 빠지지 않고 확실히 전속력 런(fast move)을 실행할 수 있도록 분대원 전체의 오더와 행동 상태를 동시에 개편 주입
                                            for member_idx in squad.members() {
                                                messages.push(RunnerMessage::BattleState(
                                                    BattleStateMessage::Soldier(
                                                        *member_idx,
                                                        SoldierMessage::SetOrder(final_move_order.clone()),
                                                    )
                                                ));
                                                messages.push(RunnerMessage::BattleState(
                                                    BattleStateMessage::Soldier(
                                                        *member_idx,
                                                        SoldierMessage::SetBehavior(Behavior::MoveFastTo(world_paths.clone())),
                                                    )
                                                ));
                                                messages.push(RunnerMessage::BattleState(
                                                    BattleStateMessage::Soldier(
                                                        *member_idx,
                                                        SoldierMessage::SetGesture(battle_core::behavior::gesture::Gesture::Idle),
                                                    )
                                                ));
                                            }
                                        }
                                    }
                                } else {
                                    // 맵 상에 갈 수 있는 중립/적 거점이 전혀 없는 극한의 예외 케이스인 경우에만 기존 대기 오더 폴백 적용
                                    for member_idx in squad.members() {
                                        messages.push(RunnerMessage::BattleState(
                                            BattleStateMessage::Soldier(
                                                *member_idx,
                                                SoldierMessage::SetOrder(Order::Idle),
                                            )
                                        ));
                                        messages.push(RunnerMessage::BattleState(
                                            BattleStateMessage::Soldier(
                                                *member_idx,
                                                SoldierMessage::SetBehavior(Behavior::Idle(Body::Crouched)),
                                            )
                                        ));
                                        messages.push(RunnerMessage::BattleState(
                                            BattleStateMessage::Soldier(
                                                *member_idx,
                                                SoldierMessage::SetGesture(battle_core::behavior::gesture::Gesture::Idle),
                                            )
                                        ));
                                    }
                                }
                            }
                        }
                    }
                } else {
                    flag_capture_data.remove(&flag_name.0);
                }

                if ownership != &new_ownership {
                    // 소유권이 변경되었을 때, 쿨다운 중이던 깃발을 다른 분대나 적이 점유했는지 확인하여 쿨다운 해제
                    if new_ownership == FlagOwnership::A || new_ownership == FlagOwnership::B {
                        let capturing_squad = if new_ownership == FlagOwnership::A {
                            a_squads_inside.first().copied()
                        } else {
                            b_squads_inside.first().copied()
                        };

                        if let Some(sq) = capturing_squad {
                            if let Some((_, cooled_sq)) = flag_cooldown_data.get(&flag_name.0) {
                                if *cooled_sq != sq {
                                    println!("[거점 순환 쿨다운 해제] 거점 [{}]가 다른 분대/진영에 의해 점유되어 기존 쿨다운이 해제됩니다.", flag_name.0);
                                    flag_cooldown_data.remove(&flag_name.0);
                                }
                            }
                        }

                        let capturing_side = if new_ownership == FlagOwnership::A { Side::A } else { Side::B };
                        for soldier in self.battle_state.soldiers() {
                            if soldier.side() == &capturing_side && soldier.alive() {
                                messages.push(RunnerMessage::BattleState(
                                    BattleStateMessage::Soldier(
                                        soldier.uuid(),
                                        SoldierMessage::RelieveStress(200)
                                    )
                                ));

                                // [자동 사격 해제 로직] 해당 깃발 근처를 타겟으로 제압 사격 중이던 아군은 사격을 중지하고 대기 상태로 전환
                                if let Order::SuppressFire(target_point) = soldier.order() {
                                    let dist = distance_between_points(&flag.position(), target_point);
                                    if dist.meters() <= 30 {
                                        messages.push(RunnerMessage::BattleState(
                                            BattleStateMessage::Soldier(
                                                soldier.uuid(),
                                                SoldierMessage::SetOrder(Order::Idle)
                                            )
                                        ));
                                        messages.push(RunnerMessage::BattleState(
                                            BattleStateMessage::Soldier(
                                                soldier.uuid(),
                                                SoldierMessage::SetBehavior(Behavior::Idle(Body::Crouched))
                                            )
                                        ));
                                    }
                                }

                                // [기획 반영: 깃발 점령 완료 시 체크포인트 복귀]
                                // 거점 점령에 기여한 병사(또는 깃발 반경 40m 내에 있는 해당 진영 병사)가 
                                // 사전에 저장해 둔 체크포인트(출발선)를 가지고 있다면, 즉시 후퇴하여 복귀하도록 명령합니다.
                                let dist_to_flag = distance_between_points(&flag.position(), &soldier.world_point());
                                if dist_to_flag.meters() <= 40 {
                                    if self.soldier_is_squad_leader(soldier.uuid()) {
                                        // [Phase 3: 거점 점령 시 턴 패스 지역화] 
                                        // 진영 전체(글로벌 턴)를 넘기지 않고, 거점 점령에 기여한 '해당 중대 내부의 분대 로테이션'만 교체합니다.
                                        let current_frame = *self.battle_state.frame_i();

                                        for (comp_name, comp) in &self.companies {
                                            if comp.scout_squad == Some(soldier.squad_uuid()) {
                                                let mut sorted_squads = comp.squads.clone();
                                                sorted_squads.sort_by(|a, b| a.0.cmp(&b.0));
                                                let cluster_anchor_key = format!("{}-group-{}", comp.side, sorted_squads[0].0);

                                                let mut offsets = self.scout_turn_offsets.write().unwrap();
                                                
                                                // 중대 내부 분대 턴 패스
                                                let entry = offsets.entry(cluster_anchor_key.clone()).or_insert((0, 0));
                                                if current_frame > entry.1 + 180 {
                                                    entry.0 += 1;
                                                    entry.1 = current_frame;
                                                    
                                                    // 임무를 성공적으로 완수한 분대를 블랙리스트(history)에 등록하여 독점을 차단
                                                    let mut history_guard = self.scouted_history.write().unwrap();
                                                    let current_history = history_guard.entry(cluster_anchor_key).or_insert_with(std::collections::HashSet::new);
                                                    current_history.insert(soldier.squad_uuid());

                                                    println!("[로테이션 지역화] 깃발 점령 완료! 중대 {} 내부의 정찰조를 교체합니다.", comp_name);
                                                }
                                            }
                                        }
                                    }

                                    if let Some(checkpoint_pos) = self.checkpoints.read().unwrap().get(&soldier.squad_uuid()) {
                                        // 수정: A* 연산 폭주(프리징) 방지 및 분대 결속 유지를 위해 분대장에게만 복귀 명령을 하달합니다.
                                        if self.soldier_is_squad_leader(soldier.uuid()) {
                                            let map = self.battle_state.map();
                                            let from_grid = map.grid_point_from_world_point(&soldier.world_point());
                                            let to_grid = map.grid_point_from_world_point(checkpoint_pos);
                                            
                                            if from_grid != to_grid {
                                                if let Some(grid_path) = battle_core::physics::path::find_path(
                                                    &self.config, map, &from_grid, &to_grid, true, &battle_core::physics::path::PathMode::Walk, &None
                                                ) {
                                                    let world_path = grid_path.iter().map(|p| map.world_point_from_grid_point(*p)).collect();
                                                    let paths = battle_core::types::WorldPaths::new(vec![battle_core::types::WorldPath::new(world_path)]);
                                                    
                                                    // [버그 수정: 복귀 오더 하달 시 분대 데드락(정지) 해결]
                                                    // 여기서 Behavior를 강제로 SetBehavior 해버리면 다음 프레임에서 상태 변경(Change)이 감지되지 않아
                                                    // 부하들에게 복귀 오더가 전파(Propagate)되지 않고 지휘관만 빠져나가는 심각한 고착 버그가 발생합니다.
                                                    // 오직 SetOrder만 하달하여 엔진이 자연스럽게 부하들에게 명령을 전파하도록 수정합니다.
                                                    messages.push(RunnerMessage::BattleState(
                                                        BattleStateMessage::Soldier(
                                                            soldier.uuid(),
                                                            SoldierMessage::SetOrder(Order::MoveFastTo(paths.clone(), Some(Box::new(Order::Idle))))
                                                        )
                                                    ));
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }

                new_ownerships.push((flag_name.clone(), new_ownership));
            }
            
            // [수정] 깃발의 소유권이 이전과 다르게 '실제로 변경'되었을 때만 상태 변경 이벤트를 발생시킵니다.
            // 이렇게 하면 매번 tick 마다 페이즈 폴더가 무한정 생성되는 것을 방지할 수 있습니다.
            if self.battle_state.flags().ownerships() != &new_ownerships {
                messages.push(RunnerMessage::BattleState(
                    BattleStateMessage::SetFlagsOwnership(FlagsOwnership::new(new_ownerships)),
                ));
            }
            
            return messages;
        }

        vec![]
    }
}
