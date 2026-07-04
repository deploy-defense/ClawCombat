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
                                messages.push(EngineMessage::SendChatCommand(final_cmd));
                                messages.push(EngineMessage::GuiState(GuiStateMessage::SelectTemplate(None)));
                                messages.push(EngineMessage::GuiState(GuiStateMessage::ToggleChatGui));
                            }
                        }
                    } else {
                        ui.label("검색된 추천 전술이 없습니다.");
                    }
                } else {
                    // 입력값이 없을 경우 기존처럼 고정 프리셋 노출
                    ui.horizontal(|ui| {
                        if ui.button("즉각 대응 사격").clicked() {
                            messages.push(EngineMessage::GuiState(GuiStateMessage::SelectTemplate(Some("suppress_fire".to_string()))));
                        }
                        if ui.button("은밀 우회 기동").clicked() {
                            messages.push(EngineMessage::GuiState(GuiStateMessage::SelectTemplate(Some("sneak_flank".to_string()))));
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
        // [버그 수정] 다중 Task가 동일한 분대를 참조할 때, 하나를 취소(Idle 강제 전환)하면 
        // 나머지 Task들도 연쇄적으로 자동 삭제되는 치명적 버그를 해결하기 위해 
        // 섣부른 자동 해제 로직을 전면 제거하고 플레이어의 수동 취소에 의존합니다.
        
        if self.gui_state.chat_tasks.is_empty() {
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