use ggez::{
    graphics::{Canvas, DrawParam},
    Context, GameResult,
};
use glam::Vec2;

use super::{message::{EngineMessage, GuiStateMessage}, Engine};

pub const EGUI_SCALE: f32 = 1.5;

fn load_system_korean_font() -> Vec<u8> {
    let paths = if cfg!(target_os = "windows") {
        vec!["C:\\Windows\\Fonts\\malgun.ttf"]
    } else if cfg!(target_os = "macos") {
        vec![
            "/System/Library/Fonts/AppleSDGothicNeo.ttc",
            "/Library/Fonts/AppleGothic.ttf",
        ]
    } else {
        vec![
            "/usr/share/fonts/truetype/nanum/NanumGothic.ttf",
            "/usr/share/fonts/truetype/noto/NotoSansCJK-Regular.ttc",
            "/usr/share/fonts/noto-cjk/NotoSansCJK-Regular.ttc",
        ]
    };

    for path in paths {
        if let Ok(bytes) = std::fs::read(path) {
            return bytes;
        }
    }
    
    panic!("시스템 폰트를 찾을 수 없습니다.");
}

impl Engine {
    pub fn draw_egui(&mut self, _ctx: &mut Context, canvas: &mut Canvas) {
        canvas.draw(
            &self.egui_backend,
            DrawParam::default().dest(Vec2::new(0., 0.)),
        );
    }

    pub fn update_chat_gui(&mut self, ctx: &mut Context) -> GameResult<()> {
        // 채팅창이 닫혀 있으면 템플릿 UI도 렌더링하지 않습니다.
        if !self.gui_state.display_chat_gui() {
            return Ok(());
        }

        let drawable_size = ctx.gfx.drawable_size();
        self.egui_backend.set_scale_factor(EGUI_SCALE, drawable_size);
        let egui_ctx = self.egui_backend.inner.ctx();
        
        // 컴파일 에러 해결을 위해 definitions() 미지원 API를 메모리 스토리지 상태 검사로 안전하게 우회
        let needs_font_init = egui_ctx.memory(|mem| {
            let is_init = mem.data.get_temp::<bool>(ggegui::egui::Id::new("korean_font_loaded")).unwrap_or(false);
            !is_init
        });

        if needs_font_init {
            let mut fonts = ggegui::egui::FontDefinitions::default();
            
            // 1. 운영체제(OS)별 자체 한글 폰트 동적 로드 (실패 시 내장 폰트 Fallback)
            fonts.font_data.insert(
                "korean_font".to_owned(),
                ggegui::egui::FontData::from_owned(load_system_korean_font()),
            );

            // 2. Proportional 및 Monospace 모두 한글 폰트를 최우선(0순위)으로 강제 할당
            fonts.families.get_mut(&ggegui::egui::FontFamily::Proportional)
                .unwrap()
                .insert(0, "korean_font".to_owned());
            fonts.families.get_mut(&ggegui::egui::FontFamily::Monospace)
                .unwrap()
                .insert(0, "korean_font".to_owned());
            
            egui_ctx.set_fonts(fonts);

            // 3. 글로벌 텍스트 스타일 덮어쓰기 (모든 스타일을 안전한 Proportional/Monospace로 롤백)
            let mut style = (*egui_ctx.style()).clone();
            if let Some(text_style) = style.text_styles.get_mut(&ggegui::egui::TextStyle::Heading) {
                text_style.family = ggegui::egui::FontFamily::Proportional;
            }
            if let Some(text_style) = style.text_styles.get_mut(&ggegui::egui::TextStyle::Body) {
                text_style.family = ggegui::egui::FontFamily::Proportional;
            }
            if let Some(text_style) = style.text_styles.get_mut(&ggegui::egui::TextStyle::Monospace) {
                text_style.family = ggegui::egui::FontFamily::Monospace;
            }
            if let Some(text_style) = style.text_styles.get_mut(&ggegui::egui::TextStyle::Button) {
                text_style.family = ggegui::egui::FontFamily::Proportional;
            }
            if let Some(text_style) = style.text_styles.get_mut(&ggegui::egui::TextStyle::Small) {
                text_style.family = ggegui::egui::FontFamily::Proportional;
            }
            egui_ctx.set_style(style);

            egui_ctx.memory_mut(|mem| {
                mem.data.insert_temp(ggegui::egui::Id::new("korean_font_loaded"), true);
            });
        }

        let mut messages = vec![];

        ggegui::egui::Window::new("전술 템플릿 제어")
            .collapsible(false)
            .resizable(false)
            // 배율 오류가 가변적인 window_size 하드코딩 pos2 연산을 전면 폐기합니다.
            // 캔버스 이탈을 원천 방지하기 위해 egui 규격 앵커 매커니즘을 사용해 안전 자리에 안착시킵니다.
            .anchor(ggegui::egui::Align2::CENTER_BOTTOM, ggegui::egui::vec2(0.0, -220.0))
            .show(&egui_ctx, |ui| {
                ui.label("현재 상황 및 상대방에 대응할 전술 템플릿을 선택하세요.");
                ui.separator();

                let chat_text = self.gui_state.chat_input().trim();

                // 입력값이 존재할 경우, 코사인 유사도 기반 자동완성 리스트 표출
                if !chat_text.is_empty() {
                    let results = &self.gui_state.tactic_suggestions;
                    if !results.is_empty() {
                        ui.label("💡 추천 전술 (자동완성):");
                        for (id, name, score) in results {
                            let btn_text = format!("{} (유사도: {:.2})", name, score);
                            // 버튼 클릭 시 해당 작전을 즉시 실행 (전술 확정)
                            if ui.button(btn_text).clicked() {
                                let current_chat = self.gui_state.chat_input().trim();
                                let final_cmd = if current_chat.is_empty() {
                                    id.clone()
                                } else {
                                    format!("{} {}", current_chat, id)
                                };
                                
                                // 자동완성 선택 시 대상 분대 자동 추출
                                let tokens: Vec<&str> = final_cmd.split_whitespace().collect();
                                let mut target_squads = vec![];
                                for t in &tokens {
                                    if t.starts_with('@') {
                                        let s = t.trim_start_matches('@').trim_end_matches("분대");
                                        if let Ok(id) = s.parse::<usize>() {
                                            target_squads.push(battle_core::types::SquadUuid(id));
                                        }
                                    }
                                }
                                
                                // 대상 분대가 없으면 현재 선택된 분대 사용
                                if target_squads.is_empty() {
                                    let selected = self.gui_state.selected_squads();
                                    if !selected.1.is_empty() {
                                        target_squads = selected.1.clone();
                                    } else {
                                        // 전체 아군 분대
                                        for squad_uuid in self.battle_state.squads().keys() {
                                            let leader_idx = self.battle_state.squad(*squad_uuid).leader();
                                            if self.battle_state.soldier(leader_idx).side() == &battle_core::game::Side::A {
                                                target_squads.push(*squad_uuid);
                                            }
                                        }
                                    }
                                }
                                
                                // Task 즉시 GUI에 등록 (채팅창 닫히기 전에)
                                if !target_squads.is_empty() {
                                    messages.push(EngineMessage::GuiState(GuiStateMessage::AddChatTask(final_cmd.clone(), target_squads)));
                                }
                                
                                messages.push(EngineMessage::SendChatCommand(final_cmd));
                                messages.push(EngineMessage::GuiState(GuiStateMessage::SelectTemplate(None)));
                                messages.push(EngineMessage::GuiState(GuiStateMessage::ToggleChatGui));
                            }
                        }
                    } else {
                        ui.label("검색된 추천 전술이 없습니다.");
                    }
                } else {
                    // 입력값이 없을 경우 동적으로 불러온 템플릿 목록(없으면 고정 프리셋) 노출
                    ui.horizontal(|ui| {
                        let templates = self.gui_state.available_templates.clone();
                        if templates.is_empty() {
                            if ui.button("즉각 대응 사격").clicked() {
                                messages.push(EngineMessage::GuiState(GuiStateMessage::SelectTemplate(Some("suppress_fire".to_string()))));
                            }
                            if ui.button("은밀 우회 기동").clicked() {
                                messages.push(EngineMessage::GuiState(GuiStateMessage::SelectTemplate(Some("sneak_flank".to_string()))));
                            }
                        } else {
                            for template in templates {
                                if ui.button(&template).clicked() {
                                    messages.push(EngineMessage::GuiState(GuiStateMessage::SelectTemplate(Some(template.clone()))));
                                }
                            }
                        }
                        
                        if ui.button("취소").clicked() {
                            messages.push(EngineMessage::GuiState(GuiStateMessage::SelectTemplate(None)));
                        }
                    });

                    // 특정 템플릿을 수동으로 클릭(선택)했을 때만 확정 메뉴를 펼쳐줍니다.
                    if let Some(selected) = self.gui_state.selected_template_to_confirm.clone() {
                        ui.separator();
                        ui.label(format!("선택된 템플릿: [{}]", selected));
                        ui.label("적의 전술을 무력화(Counter)할 수 있는지 확인 후 확정하십시오.");
                        
                        if ui.button("✔ 전술 확정 및 실행").clicked() {
                            // 확정 시 엔진에 넘기기 전에 대상 분대를 추출하여 Task UI 큐에 등록
                            let mut target_squads = vec![];
                            let selected_squads = self.gui_state.selected_squads();
                            
                            if !selected_squads.1.is_empty() {
                                target_squads = selected_squads.1.clone();
                            } else {
                                // 전체 아군 분대 스캔
                                for squad_uuid in self.battle_state.squads().keys() {
                                    let leader_idx = self.battle_state.squad(*squad_uuid).leader();
                                    if self.battle_state.soldier(leader_idx).side() == &battle_core::game::Side::A {
                                        target_squads.push(*squad_uuid);
                                    }
                                }
                            }

                            if !target_squads.is_empty() {
                                messages.push(EngineMessage::GuiState(GuiStateMessage::AddChatTask(selected.clone(), target_squads)));
                            }

                            // 확정 시 엔진에 넘기고 상태를 초기화합니다.
                            messages.push(EngineMessage::SendChatCommand(selected.clone()));
                            messages.push(EngineMessage::GuiState(GuiStateMessage::SelectTemplate(None)));
                            messages.push(EngineMessage::GuiState(GuiStateMessage::ToggleChatGui));
                        }
                    }
                }
            });

        if !messages.is_empty() {
            self.react(messages, ctx)?;
        }

        Ok(())
    }

    pub fn update_task_gui(&mut self, ctx: &mut Context) -> GameResult<()> {
        // [추가] 각 Task에 할당된 분대가 전부 소멸(사망)하거나, 모든 분대원이 Idle(대기) 상태로 돌아갔는지 검사하여 대기열에서 자동 제거합니다.
        let mut tasks_to_retain = vec![];
        for task in &self.gui_state.chat_tasks {
            let mut any_squad_active_or_alive = false;
            for squad_id in &task.2 {
                if let Some(squad) = self.battle_state.squads().get(squad_id) {
                    let active_members_count = squad.members().iter().filter(|&&m| {
                        if m.0 < self.battle_state.soldiers().len() {
                            let soldier = self.battle_state.soldier(m);
                            // 살아서 무언가 명령을 수행 중(Idle이 아님)일 때만 Active로 간주
                            soldier.alive() && !matches!(soldier.order(), battle_core::order::Order::Idle)
                        } else {
                            false
                        }
                    }).count();
                    if active_members_count > 0 {
                        any_squad_active_or_alive = true;
                        break;
                    }
                }
            }
            if any_squad_active_or_alive {
                tasks_to_retain.push(task.clone());
            }
        }
        self.gui_state.chat_tasks = tasks_to_retain;

        // Task가 비어있어도 UI는 표시하지 않지만, Task 추가 시 display_task_gui 플래그가 true인지 확인
        if self.gui_state.chat_tasks.is_empty() {
            return Ok(());
        }
        
        // Task UI는 항상 표시 (display_task_gui 플래그 확인)
        if !self.gui_state.display_task_gui {
            return Ok(());
        }

        let drawable_size = ctx.gfx.drawable_size();
        self.egui_backend.set_scale_factor(EGUI_SCALE, drawable_size);
        let egui_ctx = self.egui_backend.inner.ctx();
        
        let needs_font_init = egui_ctx.memory(|mem| {
            let is_init = mem.data.get_temp::<bool>(ggegui::egui::Id::new("korean_font_loaded")).unwrap_or(false);
            !is_init
        });

        if needs_font_init {
            let mut fonts = ggegui::egui::FontDefinitions::default();
            
            fonts.font_data.insert(
                "korean_font".to_owned(),
                ggegui::egui::FontData::from_owned(load_system_korean_font()),
            );

            fonts.families.get_mut(&ggegui::egui::FontFamily::Proportional)
                .unwrap()
                .insert(0, "korean_font".to_owned());
            fonts.families.get_mut(&ggegui::egui::FontFamily::Monospace)
                .unwrap()
                .insert(0, "korean_font".to_owned());
            
            egui_ctx.set_fonts(fonts);

            let mut style = (*egui_ctx.style()).clone();
            if let Some(text_style) = style.text_styles.get_mut(&ggegui::egui::TextStyle::Heading) {
                text_style.family = ggegui::egui::FontFamily::Proportional;
            }
            if let Some(text_style) = style.text_styles.get_mut(&ggegui::egui::TextStyle::Body) {
                text_style.family = ggegui::egui::FontFamily::Proportional;
            }
            if let Some(text_style) = style.text_styles.get_mut(&ggegui::egui::TextStyle::Monospace) {
                text_style.family = ggegui::egui::FontFamily::Monospace;
            }
            if let Some(text_style) = style.text_styles.get_mut(&ggegui::egui::TextStyle::Button) {
                text_style.family = ggegui::egui::FontFamily::Proportional;
            }
            if let Some(text_style) = style.text_styles.get_mut(&ggegui::egui::TextStyle::Small) {
                text_style.family = ggegui::egui::FontFamily::Proportional;
            }
            egui_ctx.set_style(style);

            egui_ctx.memory_mut(|mem| {
                mem.data.insert_temp(ggegui::egui::Id::new("korean_font_loaded"), true);
            });
        }

        let mut messages = vec![];

        ggegui::egui::Window::new("전술 명령 대기열 (Task List)")
            .collapsible(false)
            .resizable(false)
            .anchor(ggegui::egui::Align2::LEFT_TOP, ggegui::egui::vec2(10.0, 50.0))
            .show(&egui_ctx, |ui| {
                // [수정] 각 Task별로 독립적인 취소 처리를 위해 HashMap 사용
                let mut to_remove = None;
                let mut current_tasks = self.gui_state.chat_tasks.clone();
                
                for (idx, (id, cmd, squads)) in current_tasks.iter().enumerate() {
                    ui.horizontal(|ui| {
                        ui.label(format!("명령: {}", cmd));
                        // [수정] 고유한 버튼 ID를 위해 id를 직접 사용
                        if ui.button(format!("❌ 취소##{}", id)).clicked() {
                            // [수정] 클릭된 Task의 인덱스와 ID를 저장
                            to_remove = Some((idx, *id, squads.clone()));
                        }
                    });
                }
             
                if let Some((idx, remove_id, target_squads)) = to_remove {
                    // [수정] 특정 Task만 제거하도록 인덱스 사용
                    messages.push(EngineMessage::GuiState(GuiStateMessage::RemoveChatTask(remove_id)));

                    // [수정] target_squads가 비어있지 않은 경우에만 처리
                    if !target_squads.is_empty() {
                        for sq_id in target_squads {
                            if let Some(squad) = self.battle_state.squads().get(&sq_id) {
                                for member_idx in squad.members() {
                                    messages.push(EngineMessage::BattleState(
                                        battle_core::state::battle::message::BattleStateMessage::Soldier(
                                            *member_idx,
                                            battle_core::state::battle::message::SoldierMessage::SetOrder(battle_core::order::Order::Idle)
                                        )
                                    ));
                                    messages.push(EngineMessage::BattleState(
                                        battle_core::state::battle::message::BattleStateMessage::Soldier(
                                            *member_idx,
                                            battle_core::state::battle::message::SoldierMessage::SetBehavior(
                                                battle_core::behavior::Behavior::Idle(battle_core::behavior::Body::Crouched)
                                            )
                                        )
                                    ));
                                    messages.push(EngineMessage::BattleState(
                                        battle_core::state::battle::message::BattleStateMessage::Soldier(
                                            *member_idx,
                                            battle_core::state::battle::message::SoldierMessage::SetGesture(
                                                battle_core::behavior::gesture::Gesture::Idle
                                            )
                                        )
                                    ));
                                }
                            }
                        }
                    }
                }
            });

        if !messages.is_empty() {
            self.react(messages, ctx)?;
        }

        Ok(())
    }
}