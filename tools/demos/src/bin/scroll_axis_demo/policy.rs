use std::time::{Duration, Instant};

use gpui::ScrollGesturePhase;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScrollMode {
    Current,
    OsGestureLock,
    OsLockPlusEma,
    Hysteresis,
}

impl ScrollMode {
    pub const ALL: [Self; 4] = [
        Self::Current,
        Self::OsGestureLock,
        Self::OsLockPlusEma,
        Self::Hysteresis,
    ];

    pub fn short_label(self) -> &'static str {
        match self {
            Self::Current => "Current (Stateless)",
            Self::OsGestureLock => "A · OS Gesture Lock",
            Self::OsLockPlusEma => "B · OS Lock + EMA",
            Self::Hysteresis => "C · Hysteresis",
        }
    }

    pub fn summary(self) -> &'static str {
        match self {
            Self::Current => {
                "現行: 1イベント毎に |dx| >= |dy| を瞬間判定。縦スクロール中の微妙な横ブレで横にも反応。"
            }
            Self::OsGestureLock => {
                "マジックナンバー0個: 指置き直し(Began)で即解除。1ジェスチャ〜慣性終了まで最初の軸を維持。"
            }
            Self::OsLockPlusEma => {
                "パラメータ1個(EMA): 慣性はロック＆Beganで即解除。指を離さずL字に曲げても滑らかに切替。"
            }
            Self::Hysteresis => {
                "参考比較: ロック中に |dx| > |dy|*2 が14px連続で突破。120ms無操作またはBeganで解除。"
            }
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScrollAxis {
    Vertical,
    Horizontal,
}

impl ScrollAxis {
    pub fn label(self) -> &'static str {
        match self {
            Self::Vertical => "VERTICAL",
            Self::Horizontal => "HORIZONTAL",
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct RouteDecision {
    pub applied_x: f32,
    pub applied_y: f32,
    pub locked: Option<ScrollAxis>,
}

pub struct ScrollAxisRouter {
    locked: Option<ScrollAxis>,
    ema_x: f32,
    ema_y: f32,
    break_accum: f32,
    last_event_at: Option<Instant>,
    pub last_phase: ScrollGesturePhase,
    pub last_raw: (f32, f32),
    pub last_applied: (f32, f32),
    pub axis_switches: u32,
}

impl ScrollAxisRouter {
    const EMA_ALPHA: f32 = 0.25;
    const HYSTERESIS_RATIO: f32 = 2.0;
    const HYSTERESIS_BREAK_PX: f32 = 14.0;
    const IDLE_RESET_MS: u64 = 120;

    pub fn new() -> Self {
        Self {
            locked: None,
            ema_x: 0.0,
            ema_y: 0.0,
            break_accum: 0.0,
            last_event_at: None,
            last_phase: ScrollGesturePhase::Discrete,
            last_raw: (0.0, 0.0),
            last_applied: (0.0, 0.0),
            axis_switches: 0,
        }
    }

    pub fn reset(&mut self) {
        self.locked = None;
        self.ema_x = 0.0;
        self.ema_y = 0.0;
        self.break_accum = 0.0;
        self.last_event_at = None;
        self.axis_switches = 0;
        self.last_raw = (0.0, 0.0);
        self.last_applied = (0.0, 0.0);
    }

    pub fn locked(&self) -> Option<ScrollAxis> {
        self.locked
    }

    pub fn route(
        &mut self,
        mode: ScrollMode,
        phase: ScrollGesturePhase,
        dx: f32,
        dy: f32,
        now: Instant,
    ) -> RouteDecision {
        let prev_axis = self.locked;
        self.last_phase = phase;
        self.last_raw = (dx, dy);

        let decision = match mode {
            ScrollMode::Current => self.route_current(dx, dy),
            ScrollMode::OsGestureLock => self.route_os_gesture_lock(phase, dx, dy),
            ScrollMode::OsLockPlusEma => self.route_os_lock_ema(phase, dx, dy),
            ScrollMode::Hysteresis => self.route_hysteresis(phase, dx, dy, now),
        };

        if let (Some(prev), Some(next)) = (prev_axis, decision.locked)
            && prev != next
            && !matches!(phase, ScrollGesturePhase::Began | ScrollGesturePhase::Discrete)
        {
            self.axis_switches += 1;
        }
        self.last_event_at = Some(now);
        self.last_applied = (decision.applied_x, decision.applied_y);
        decision
    }

    fn route_current(&mut self, dx: f32, dy: f32) -> RouteDecision {
        let ax = dx.abs();
        let ay = dy.abs();
        if ax == 0.0 && ay == 0.0 {
            self.locked = None;
            return RouteDecision {
                applied_x: 0.0,
                applied_y: 0.0,
                locked: None,
            };
        }
        if ax >= ay {
            self.locked = Some(ScrollAxis::Horizontal);
            RouteDecision {
                applied_x: dx,
                applied_y: 0.0,
                locked: self.locked,
            }
        } else {
            self.locked = Some(ScrollAxis::Vertical);
            RouteDecision {
                applied_x: 0.0,
                applied_y: dy,
                locked: self.locked,
            }
        }
    }

    fn route_os_gesture_lock(
        &mut self,
        phase: ScrollGesturePhase,
        dx: f32,
        dy: f32,
    ) -> RouteDecision {
        match phase {
            ScrollGesturePhase::Began => {
                self.locked = None;
            }
            ScrollGesturePhase::MomentumEnded => {
                self.locked = None;
                return RouteDecision {
                    applied_x: 0.0,
                    applied_y: 0.0,
                    locked: None,
                };
            }
            ScrollGesturePhase::Discrete => {
                self.locked = None;
                return self.apply_single_axis(dx, dy);
            }
            ScrollGesturePhase::Changed
            | ScrollGesturePhase::Ended
            | ScrollGesturePhase::Momentum => {}
        }

        let ax = dx.abs();
        let ay = dy.abs();
        if self.locked.is_none() && (ax > 0.0 || ay > 0.0) {
            self.locked = Some(if ax > ay {
                ScrollAxis::Horizontal
            } else {
                ScrollAxis::Vertical
            });
        }

        self.apply_locked(dx, dy)
    }

    fn route_os_lock_ema(
        &mut self,
        phase: ScrollGesturePhase,
        dx: f32,
        dy: f32,
    ) -> RouteDecision {
        match phase {
            ScrollGesturePhase::Began => {
                self.locked = None;
                self.ema_x = 0.0;
                self.ema_y = 0.0;
            }
            ScrollGesturePhase::MomentumEnded => {
                self.locked = None;
                self.ema_x = 0.0;
                self.ema_y = 0.0;
                return RouteDecision {
                    applied_x: 0.0,
                    applied_y: 0.0,
                    locked: None,
                };
            }
            ScrollGesturePhase::Discrete => {
                self.locked = None;
                self.ema_x = 0.0;
                self.ema_y = 0.0;
                return self.apply_single_axis(dx, dy);
            }
            ScrollGesturePhase::Momentum => {
                let ax = dx.abs();
                let ay = dy.abs();
                if self.locked.is_none() && (ax > 0.0 || ay > 0.0) {
                    self.locked = Some(if ax > ay {
                        ScrollAxis::Horizontal
                    } else {
                        ScrollAxis::Vertical
                    });
                }
                return self.apply_locked(dx, dy);
            }
            ScrollGesturePhase::Changed | ScrollGesturePhase::Ended => {}
        }

        let ax = dx.abs();
        let ay = dy.abs();
        if ax > 0.0 || ay > 0.0 {
            if self.locked.is_none() {
                self.ema_x = ax;
                self.ema_y = ay;
            } else {
                self.ema_x = self.ema_x * (1.0 - Self::EMA_ALPHA) + ax * Self::EMA_ALPHA;
                self.ema_y = self.ema_y * (1.0 - Self::EMA_ALPHA) + ay * Self::EMA_ALPHA;
            }
            self.locked = Some(if self.ema_x > self.ema_y {
                ScrollAxis::Horizontal
            } else {
                ScrollAxis::Vertical
            });
        }

        self.apply_locked(dx, dy)
    }

    fn route_hysteresis(
        &mut self,
        phase: ScrollGesturePhase,
        dx: f32,
        dy: f32,
        now: Instant,
    ) -> RouteDecision {
        let timed_out = self
            .last_event_at
            .is_some_and(|t| now.saturating_duration_since(t) > Duration::from_millis(Self::IDLE_RESET_MS));

        match phase {
            ScrollGesturePhase::Began => {
                self.locked = None;
                self.break_accum = 0.0;
            }
            ScrollGesturePhase::MomentumEnded => {
                self.locked = None;
                self.break_accum = 0.0;
                return RouteDecision {
                    applied_x: 0.0,
                    applied_y: 0.0,
                    locked: None,
                };
            }
            ScrollGesturePhase::Discrete => {
                self.locked = None;
                self.break_accum = 0.0;
                return self.apply_single_axis(dx, dy);
            }
            ScrollGesturePhase::Momentum => {
                return self.apply_locked(dx, dy);
            }
            ScrollGesturePhase::Changed | ScrollGesturePhase::Ended => {
                if timed_out {
                    self.locked = None;
                    self.break_accum = 0.0;
                }
            }
        }

        let ax = dx.abs();
        let ay = dy.abs();
        match self.locked {
            None if ax > 0.0 || ay > 0.0 => {
                self.locked = Some(if ax > ay {
                    ScrollAxis::Horizontal
                } else {
                    ScrollAxis::Vertical
                });
                self.break_accum = 0.0;
            }
            Some(ScrollAxis::Vertical) => {
                if ax > ay * Self::HYSTERESIS_RATIO && ax > 0.0 {
                    self.break_accum += ax;
                    if self.break_accum >= Self::HYSTERESIS_BREAK_PX {
                        self.locked = Some(ScrollAxis::Horizontal);
                        self.break_accum = 0.0;
                    }
                } else {
                    self.break_accum = (self.break_accum - ay).max(0.0);
                }
            }
            Some(ScrollAxis::Horizontal) => {
                if ay > ax * Self::HYSTERESIS_RATIO && ay > 0.0 {
                    self.break_accum += ay;
                    if self.break_accum >= Self::HYSTERESIS_BREAK_PX {
                        self.locked = Some(ScrollAxis::Vertical);
                        self.break_accum = 0.0;
                    }
                } else {
                    self.break_accum = (self.break_accum - ax).max(0.0);
                }
            }
            None => {}
        }

        self.apply_locked(dx, dy)
    }

    fn apply_single_axis(&self, dx: f32, dy: f32) -> RouteDecision {
        if dx.abs() > dy.abs() {
            RouteDecision {
                applied_x: dx,
                applied_y: 0.0,
                locked: Some(ScrollAxis::Horizontal),
            }
        } else if dy != 0.0 {
            RouteDecision {
                applied_x: 0.0,
                applied_y: dy,
                locked: Some(ScrollAxis::Vertical),
            }
        } else {
            RouteDecision {
                applied_x: 0.0,
                applied_y: 0.0,
                locked: None,
            }
        }
    }

    fn apply_locked(&self, dx: f32, dy: f32) -> RouteDecision {
        match self.locked {
            Some(ScrollAxis::Horizontal) => RouteDecision {
                applied_x: dx,
                applied_y: 0.0,
                locked: self.locked,
            },
            Some(ScrollAxis::Vertical) => RouteDecision {
                applied_x: 0.0,
                applied_y: dy,
                locked: self.locked,
            },
            None => RouteDecision {
                applied_x: 0.0,
                applied_y: 0.0,
                locked: None,
            },
        }
    }
}

pub fn phase_label(phase: ScrollGesturePhase) -> &'static str {
    match phase {
        ScrollGesturePhase::Discrete => "Discrete",
        ScrollGesturePhase::Began => "Began",
        ScrollGesturePhase::Changed => "Changed",
        ScrollGesturePhase::Ended => "Ended",
        ScrollGesturePhase::Momentum => "Momentum",
        ScrollGesturePhase::MomentumEnded => "MomEnd",
    }
}
