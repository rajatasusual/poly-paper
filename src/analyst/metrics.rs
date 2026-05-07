use crate::analyst::models::Session;

pub struct SessionMetrics {
    pub pnl: f64,
    pub duration_secs: f64,
    pub execution_count: usize,
    pub peak_deployment: f64,
    pub efficiency: f64,
}

pub fn compute_metrics(session: &Session) -> SessionMetrics {
    let pnl = session.realized_pnl.parse::<f64>().unwrap_or(0.0);

    let duration_secs = (session.ended_unix_ms - session.started_unix_ms) as f64 / 1000.0;

    let mut peak = 0.0;

    for exec in &session.executions {
        let pending = exec
            .pending_settlement_payout_after
            .parse::<f64>()
            .unwrap_or(0.0);

        if pending > peak {
            peak = pending;
        }
    }

    let efficiency = if peak > 0.0 { pnl / peak } else { 0.0 };

    SessionMetrics {
        pnl,
        duration_secs,
        execution_count: session.executions.len(),
        peak_deployment: peak,
        efficiency,
    }
}
