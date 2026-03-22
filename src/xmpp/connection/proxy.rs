#![allow(dead_code)]
// Task P1.1b — Proxy lifecycle state machine
//
// Pure state machine — no async, no I/O, no networking.
// Models the TCP proxy lifecycle with WebSocket fallback after 3 consecutive failures.

#[derive(Debug, Clone, PartialEq)]
pub enum ProxyState {
    Stopped,
    Starting,
    Running,
    Stopping,
    Failed(String),
}

#[derive(Debug, Clone, PartialEq)]
pub enum TransportKind {
    TcpProxy,
    WebSocket,
}

pub struct ProxyLifecycle {
    state: ProxyState,
    /// Which transport is currently active (or will be attempted).
    transport: TransportKind,
    /// How many consecutive TCP proxy start failures we've seen.
    tcp_failures: u32,
    /// Timeout budget in milliseconds before fallback kicks in.
    start_timeout_ms: u64,
    /// When `Starting`, the elapsed time (simulated via method argument).
    elapsed_ms: u64,
}

impl ProxyLifecycle {
    pub fn new(start_timeout_ms: u64) -> Self {
        Self {
            state: ProxyState::Stopped,
            transport: TransportKind::TcpProxy,
            tcp_failures: 0,
            start_timeout_ms,
            elapsed_ms: 0,
        }
    }

    /// Request start — transitions Stopped → Starting.
    /// Returns Err if already Starting or Running.
    pub fn start(&mut self) -> Result<(), String> {
        match &self.state {
            ProxyState::Starting => Err("already starting".into()),
            ProxyState::Running => Err("already running".into()),
            _ => {
                self.elapsed_ms = 0;
                self.state = ProxyState::Starting;
                Ok(())
            }
        }
    }

    /// Report that the proxy came up successfully.
    /// Transitions Starting → Running.
    pub fn on_started(&mut self) {
        if self.state == ProxyState::Starting {
            self.state = ProxyState::Running;
        }
    }

    /// Report a start failure. Increments tcp_failures.
    /// If tcp_failures >= 3: switch transport to WebSocket, reset to Stopped.
    /// Otherwise: remain at Failed state.
    pub fn on_start_failed(&mut self, reason: String) {
        self.tcp_failures += 1;
        if self.tcp_failures >= 3 {
            self.transport = TransportKind::WebSocket;
            self.state = ProxyState::Stopped;
        } else {
            self.state = ProxyState::Failed(reason);
        }
    }

    /// Tick with elapsed ms. If Starting and elapsed > start_timeout_ms: trigger on_start_failed.
    /// Returns true if a timeout was triggered.
    pub fn tick(&mut self, elapsed_ms: u64) -> bool {
        self.elapsed_ms = elapsed_ms;
        if self.state == ProxyState::Starting && self.elapsed_ms > self.start_timeout_ms {
            self.on_start_failed("start timeout".into());
            true
        } else {
            false
        }
    }

    /// Graceful stop — transitions Running → Stopping → Stopped.
    pub fn stop(&mut self) {
        if self.state == ProxyState::Running {
            self.state = ProxyState::Stopping;
        }
    }

    /// Report stop complete.
    pub fn on_stopped(&mut self) {
        if self.state == ProxyState::Stopping {
            self.state = ProxyState::Stopped;
        }
    }

    pub fn state(&self) -> &ProxyState {
        &self.state
    }

    pub fn transport(&self) -> TransportKind {
        self.transport.clone()
    }

    pub fn tcp_failures(&self) -> u32 {
        self.tcp_failures
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_starts_stopped_with_tcp() {
        let lc = ProxyLifecycle::new(5000);
        assert_eq!(lc.state(), &ProxyState::Stopped);
        assert_eq!(lc.transport(), TransportKind::TcpProxy);
        assert_eq!(lc.tcp_failures(), 0);
    }

    #[test]
    fn start_transitions_to_starting() {
        let mut lc = ProxyLifecycle::new(5000);
        assert!(lc.start().is_ok());
        assert_eq!(lc.state(), &ProxyState::Starting);
    }

    #[test]
    fn start_fails_when_already_running() {
        let mut lc = ProxyLifecycle::new(5000);
        lc.start().unwrap();
        lc.on_started();
        assert_eq!(lc.state(), &ProxyState::Running);
        let result = lc.start();
        assert!(result.is_err());
    }

    #[test]
    fn on_started_transitions_to_running() {
        let mut lc = ProxyLifecycle::new(5000);
        lc.start().unwrap();
        lc.on_started();
        assert_eq!(lc.state(), &ProxyState::Running);
    }

    #[test]
    fn on_start_failed_increments_failures() {
        let mut lc = ProxyLifecycle::new(5000);
        lc.start().unwrap();
        lc.on_start_failed("connection refused".into());
        assert_eq!(lc.tcp_failures(), 1);
        assert_eq!(lc.state(), &ProxyState::Failed("connection refused".into()));
    }

    #[test]
    fn three_failures_falls_back_to_websocket() {
        let mut lc = ProxyLifecycle::new(5000);

        lc.start().unwrap();
        lc.on_start_failed("err".into());
        assert_eq!(lc.transport(), TransportKind::TcpProxy);

        lc.start().unwrap();
        lc.on_start_failed("err".into());
        assert_eq!(lc.transport(), TransportKind::TcpProxy);

        lc.start().unwrap();
        lc.on_start_failed("err".into());
        assert_eq!(lc.transport(), TransportKind::WebSocket);
        assert_eq!(lc.tcp_failures(), 3);
    }

    #[test]
    fn tick_triggers_timeout_and_failure() {
        let mut lc = ProxyLifecycle::new(3000);
        lc.start().unwrap();

        // under timeout — no trigger
        let triggered = lc.tick(2000);
        assert!(!triggered);
        assert_eq!(lc.state(), &ProxyState::Starting);

        // over timeout — triggers failure
        let triggered = lc.tick(3001);
        assert!(triggered);
        assert_ne!(lc.state(), &ProxyState::Starting);
    }

    #[test]
    fn stop_and_on_stopped_cycle() {
        let mut lc = ProxyLifecycle::new(5000);
        lc.start().unwrap();
        lc.on_started();
        assert_eq!(lc.state(), &ProxyState::Running);

        lc.stop();
        assert_eq!(lc.state(), &ProxyState::Stopping);

        lc.on_stopped();
        assert_eq!(lc.state(), &ProxyState::Stopped);
    }

    #[test]
    fn three_failures_resets_to_stopped() {
        let mut lc = ProxyLifecycle::new(5000);

        for _ in 0..3 {
            lc.start().unwrap();
            lc.on_start_failed("err".into());
        }

        assert_eq!(lc.state(), &ProxyState::Stopped);
        assert_eq!(lc.transport(), TransportKind::WebSocket);
    }
}
