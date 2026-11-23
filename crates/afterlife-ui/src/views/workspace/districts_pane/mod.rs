mod compact;
mod standard;

use afterlife_core::EngineCommand;
use afterlife_design::{BORDER_PX, GlassSurface, SpacePx, with_alpha};
use gpui::{
    AnyElement, Context, InteractiveElement, IntoElement, ParentElement, SharedString,
    StatefulInteractiveElement, Styled, div, px,
};

use crate::{
    app::{
        motion::{MotionSnapshot, lerp_rgba},
        state::{AppState, now_ms},
    },
    components::drag::DraggedJob,
    views::root::AfterlifeApp,
};

pub(crate) const LEFT_PANE_WIDTH_PX: f32 = 368.0;
pub(crate) const COMPACT_STAGE_MIN_WIDTH_PX: f32 = 188.0;
pub(crate) const COMPACT_STAGE_HEIGHT_PX: f32 = 114.0;
pub(crate) const MAX_COMPACT_JOBS: usize = 3;

pub(crate) fn standard_stage_width(window_width: f32) -> f32 {
    let safe_w = if window_width.is_finite() {
        window_width
    } else {
        0.0
    };
    let right_pane_w =
        (safe_w - LEFT_PANE_WIDTH_PX - SpacePx::S16.val() * 2.0 - SpacePx::S24.val()).max(600.0);
    let district_inner_w = (right_pane_w - SpacePx::S16.val() * 2.0).max(560.0);
    ((district_inner_w - SpacePx::S12.val() * 2.0) / 3.0).clamp(210.0, 440.0)
}

pub(crate) fn compact_stage_width(window_width: f32, max_stages: usize) -> f32 {
    let safe_w = if window_width.is_finite() {
        window_width.max(0.0)
    } else {
        0.0
    };
    let cols = max_stages.max(1) as f32;
    let right_pane_w =
        (safe_w - LEFT_PANE_WIDTH_PX - SpacePx::S16.val() * 2.0 - SpacePx::S24.val()).max(0.0);
    let district_inner_w = (right_pane_w - SpacePx::S8.val() * 2.0 - BORDER_PX * 2.0).max(0.0);
    let total_gaps = SpacePx::S8.val() * (cols - 1.0);
    ((district_inner_w - total_gaps) / cols)
        .floor()
        .max(COMPACT_STAGE_MIN_WIDTH_PX)
}

pub(crate) fn compact_stage_overflow(job_count: usize) -> usize {
    job_count.saturating_sub(MAX_COMPACT_JOBS)
}

pub(crate) fn max_active_district_stages(st: &AppState) -> usize {
    st.engine
        .districts
        .iter()
        .filter(|d| !d.archived)
        .map(|d| d.stages().len())
        .max()
        .unwrap_or(1)
        .max(1)
}

pub fn render_right_districts_pane(
    st: &AppState,
    motion: &MotionSnapshot,
    window_width: f32,
    cx: &mut Context<AfterlifeApp>,
) -> AnyElement {
    let p = st.palette;
    let compact = st.compact_districts;
    let stage_w = if compact {
        compact_stage_width(window_width, max_active_district_stages(st))
    } else {
        standard_stage_width(window_width)
    };

    div()
        .id("right-districts-scroll")
        .flex()
        .flex_col()
        .flex_1()
        .min_w(px(0.0))
        .h_full()
        .min_h(px(0.0))
        .overflow_y_scroll()
        .restrict_scroll_to_axis()
        .gap(if compact {
            SpacePx::S12.px()
        } else {
            SpacePx::S24.px()
        })
        .children(st.engine.districts.iter().filter(|d| !d.archived).map(|district| {
            let district_id_drop = district.id.clone();
            let district_pulse_key = format!("district-drop-{}", district.id);
            let district_pulse = motion.pulse_strength(&district_pulse_key);
            let district_bg = lerp_rgba(
                p.panel_base,
                with_alpha(p.accent_inactive, 0.32),
                district_pulse,
            );
            let district_jobs_count = st
                .engine
                .jobs
                .iter()
                .filter(|j| j.is_in_district(&district.id))
                .count();

            let panel = GlassSurface::panel(&p, &st.glass)
                .pad(if compact { SpacePx::S8 } else { SpacePx::S16 })
                .gap(if compact { SpacePx::S8 } else { SpacePx::S12 })
                .bg(Some(district_bg))
                .build()
                .w_full()
                .min_w(px(0.0))
                .flex_shrink_0()
                .id(SharedString::from(format!("district-{}", district.id)))
                .on_drop::<DraggedJob>(cx.listener(move |this, drag: &DraggedJob, _, cx| {
                    this.state.update(cx, |st, _| {
                        let _ = st.dispatch(EngineCommand::AssignJobToDistrict {
                            job_id: drag.job_id.clone(),
                            district_id: district_id_drop.clone(),
                            expected_revision: None,
                            at: now_ms(),
                        });
                        st.selected_job_id = drag.job_id.clone();
                    });
                    this.motion.trigger_pulse(district_pulse_key.clone());
                    cx.notify();
                }))
                .drag_over::<DraggedJob>(move |style, _, _, _| {
                    style.bg(with_alpha(p.accent_inactive, 0.28))
                });

            if compact {
                compact::render_compact_district(
                    panel,
                    st,
                    motion,
                    district,
                    district_jobs_count,
                    stage_w,
                    cx,
                )
            } else {
                standard::render_standard_district(
                    panel,
                    st,
                    motion,
                    district,
                    district_jobs_count,
                    stage_w,
                    cx,
                )
            }
        }))
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compact_stage_width_aligns_columns_and_enforces_minimum_floor() {
        let wide = compact_stage_width(1920.0, 7);
        assert!(wide > COMPACT_STAGE_MIN_WIDTH_PX);
        let wide_inner = 1920.0 - LEFT_PANE_WIDTH_PX - SpacePx::S16.val() * 2.0
            - SpacePx::S24.val() - SpacePx::S8.val() * 2.0 - BORDER_PX * 2.0;
        let total_wide = wide * 7.0 + SpacePx::S8.val() * 6.0;
        assert!(total_wide <= wide_inner);

        assert_eq!(compact_stage_width(1440.0, 7), COMPACT_STAGE_MIN_WIDTH_PX);
        assert_eq!(compact_stage_width(1000.0, 7), COMPACT_STAGE_MIN_WIDTH_PX);
        assert_eq!(compact_stage_width(0.0, 5), COMPACT_STAGE_MIN_WIDTH_PX);
        assert_eq!(compact_stage_width(-500.0, 5), COMPACT_STAGE_MIN_WIDTH_PX);
        assert_eq!(compact_stage_width(f32::NAN, 5), COMPACT_STAGE_MIN_WIDTH_PX);
        assert_eq!(compact_stage_width(f32::INFINITY, 5), COMPACT_STAGE_MIN_WIDTH_PX);
        assert!(compact_stage_width(1440.0, 0) >= COMPACT_STAGE_MIN_WIDTH_PX);
    }

    #[test]
    fn compact_stage_overflow_counts_only_excess_above_three_jobs() {
        assert_eq!(compact_stage_overflow(0), 0);
        assert_eq!(compact_stage_overflow(1), 0);
        assert_eq!(compact_stage_overflow(2), 0);
        assert_eq!(compact_stage_overflow(3), 0);
        assert_eq!(compact_stage_overflow(4), 1);
        assert_eq!(compact_stage_overflow(7), 4);
    }

    #[test]
    fn max_active_district_stages_ignores_archived_districts() {
        let mut st = AppState::new_ephemeral();
        let initial_max = max_active_district_stages(&st);
        assert!(initial_max >= 5);

        let eng = st.engine_mut();
        for d in &mut eng.districts {
            if d.stages().len() == initial_max {
                d.archived = true;
            }
        }
        let next_max = max_active_district_stages(&st);
        assert!(next_max <= initial_max);
        assert!(next_max >= 1);
    }
}
