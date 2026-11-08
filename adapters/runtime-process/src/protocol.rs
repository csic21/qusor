use extension_runtime_api::{
    ExecutionLimits, ExecutionResult, MAX_RAW_REQUEST_BYTES, MAX_RESPONSE_BYTES, MAX_WASM_BYTES,
    RuntimeError, RuntimeErrorKind,
};
use serde::{Deserialize, Serialize};
use std::time::Duration;

pub const PROCESS_PROTOCOL: u16 = 1;
pub(crate) const MAX_CALLS: usize = 8;
pub(crate) const MAX_FRAME: usize = 4 * MAX_RAW_REQUEST_BYTES + 4096;
pub(crate) const MAX_BOOT: usize = 4 * MAX_WASM_BYTES + 4096;

#[derive(Debug)]
pub struct FrozenExecution {
    pub module: Vec<u8>,
    pub limits: ExecutionLimits,
    pub request_limit: usize,
}
impl FrozenExecution {
    pub(crate) fn valid(&self) -> bool {
        !self.module.is_empty()
            && self.module.len() <= MAX_WASM_BYTES
            && self.limits.validate().is_ok()
            && self.limits.host_calls <= MAX_CALLS
            && self.request_limit > 0
            && self.request_limit <= MAX_RAW_REQUEST_BYTES
            && self.limits.deadline <= Duration::from_secs(10)
    }
}
#[derive(Debug)]
pub struct ExecutionFailure {
    pub kind: RuntimeErrorKind,
    pub code: String,
}
#[derive(Debug)]
pub enum WorkerOutcome {
    Completed(ExecutionResult),
    Failed(ExecutionFailure),
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Boot {
    pub protocol_version: u16,
    pub run_id: String,
    pub module: Vec<u8>,
    pub limits: WireLimits,
    pub request_limit: usize,
}
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct WireLimits {
    total_fuel: u64,
    slice_fuel: u64,
    deadline_ms: u64,
    host_calls: usize,
}
impl From<&ExecutionLimits> for WireLimits {
    fn from(v: &ExecutionLimits) -> Self {
        Self {
            total_fuel: v.total_fuel,
            slice_fuel: v.slice_fuel,
            deadline_ms: v.deadline.as_millis().try_into().unwrap_or(u64::MAX),
            host_calls: v.host_calls,
        }
    }
}
impl From<WireLimits> for ExecutionLimits {
    fn from(v: WireLimits) -> Self {
        Self {
            total_fuel: v.total_fuel,
            slice_fuel: v.slice_fuel,
            deadline: Duration::from_millis(v.deadline_ms),
            host_calls: v.host_calls,
        }
    }
}
#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub(crate) enum Message {
    GuardReady {
        protocol_version: u16,
        run_id: String,
    },
    Ready {
        run_id: String,
    },
    Call {
        run_id: String,
        call_id: u64,
        bytes: Vec<u8>,
    },
    Result {
        run_id: String,
        outcome: WireOutcome,
    },
}
impl Message {
    pub fn run_id(&self) -> &str {
        match self {
            Self::GuardReady { run_id, .. }
            | Self::Ready { run_id }
            | Self::Call { run_id, .. }
            | Self::Result { run_id, .. } => run_id,
        }
    }
}
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Reply {
    pub run_id: String,
    pub call_id: u64,
    pub bytes: Vec<u8>,
}
#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case", deny_unknown_fields)]
pub(crate) enum WireOutcome {
    Completed { result: ExecutionResult },
    Failed { failure: WireFailure },
}
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct WireFailure {
    kind: FailureKind,
    code: String,
}
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum FailureKind {
    InvalidConfiguration,
    InvalidModule,
    InvalidBuffer,
    InvalidRequest,
    BudgetExceeded,
    FuelExhausted,
    DeadlineExceeded,
    PrivateStateRejected,
    BrokerRejected,
    Trap,
}
impl From<RuntimeErrorKind> for FailureKind {
    fn from(k: RuntimeErrorKind) -> Self {
        match k {
            RuntimeErrorKind::InvalidConfiguration => Self::InvalidConfiguration,
            RuntimeErrorKind::InvalidModule => Self::InvalidModule,
            RuntimeErrorKind::InvalidBuffer => Self::InvalidBuffer,
            RuntimeErrorKind::InvalidRequest => Self::InvalidRequest,
            RuntimeErrorKind::BudgetExceeded => Self::BudgetExceeded,
            RuntimeErrorKind::FuelExhausted => Self::FuelExhausted,
            RuntimeErrorKind::DeadlineExceeded => Self::DeadlineExceeded,
            RuntimeErrorKind::PrivateStateRejected => Self::PrivateStateRejected,
            RuntimeErrorKind::BrokerRejected => Self::BrokerRejected,
            RuntimeErrorKind::Trap => Self::Trap,
        }
    }
}
impl From<FailureKind> for RuntimeErrorKind {
    fn from(k: FailureKind) -> Self {
        match k {
            FailureKind::InvalidConfiguration => Self::InvalidConfiguration,
            FailureKind::InvalidModule => Self::InvalidModule,
            FailureKind::InvalidBuffer => Self::InvalidBuffer,
            FailureKind::InvalidRequest => Self::InvalidRequest,
            FailureKind::BudgetExceeded => Self::BudgetExceeded,
            FailureKind::FuelExhausted => Self::FuelExhausted,
            FailureKind::DeadlineExceeded => Self::DeadlineExceeded,
            FailureKind::PrivateStateRejected => Self::PrivateStateRejected,
            FailureKind::BrokerRejected => Self::BrokerRejected,
            FailureKind::Trap => Self::Trap,
        }
    }
}
impl WireOutcome {
    pub fn from_execution(result: extension_runtime_api::Result<ExecutionResult>) -> Self {
        match result {
            Ok(result) => Self::Completed { result },
            Err(error) => Self::Failed {
                failure: WireFailure {
                    kind: error.kind().into(),
                    code: bounded_code(error.code),
                },
            },
        }
    }
    pub fn into_public(self) -> Option<WorkerOutcome> {
        match self {
            Self::Completed { result } if result.output.len() <= MAX_RESPONSE_BYTES => {
                Some(WorkerOutcome::Completed(result))
            }
            Self::Failed { failure } if valid_code(&failure.code) => {
                Some(WorkerOutcome::Failed(ExecutionFailure {
                    kind: failure.kind.into(),
                    code: failure.code,
                }))
            }
            _ => None,
        }
    }
}
fn bounded_code(code: &str) -> String {
    if valid_code(code) {
        code.to_owned()
    } else {
        "runtime_failure".into()
    }
}
fn valid_code(code: &str) -> bool {
    !code.is_empty()
        && code.len() <= 128
        && code
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"_-".contains(&b))
}
pub(crate) fn channel_error() -> RuntimeError {
    RuntimeError::new("process_channel_failed")
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn worst_case_raw_request_and_boot_fit_frames_without_shrinking_public_limits() {
        let msg = Message::Call {
            run_id: "r".repeat(96),
            call_id: u64::MAX,
            bytes: vec![255; MAX_RAW_REQUEST_BYTES],
        };
        let bytes = serde_json::to_vec(&msg).unwrap();
        assert!(bytes.len() < MAX_FRAME);
        let decoded: Message = serde_json::from_slice(&bytes).unwrap();
        if let Message::Call { bytes, .. } = decoded {
            assert_eq!(bytes.len(), MAX_RAW_REQUEST_BYTES);
            assert!(bytes.iter().all(|b| *b == 255));
        } else {
            panic!()
        }
        let boot = Boot {
            protocol_version: PROCESS_PROTOCOL,
            run_id: "r".repeat(96),
            module: vec![255; MAX_WASM_BYTES],
            limits: WireLimits::from(&ExecutionLimits::default()),
            request_limit: MAX_RAW_REQUEST_BYTES,
        };
        assert!(serde_json::to_vec(&boot).unwrap().len() < MAX_BOOT);
    }
    #[test]
    fn worst_case_control_character_result_fits_and_round_trips() {
        let frame = Message::Result {
            run_id: "r".repeat(96),
            outcome: WireOutcome::Completed {
                result: ExecutionResult {
                    output: "\0".repeat(MAX_RESPONSE_BYTES),
                    calls: 8,
                    fuel_consumed: 1_000_000,
                },
            },
        };
        let encoded = serde_json::to_vec(&frame).unwrap();
        assert!(encoded.len() > 6 * MAX_RESPONSE_BYTES);
        assert!(encoded.len() < MAX_FRAME);
        let decoded: Message = serde_json::from_slice(&encoded).unwrap();
        if let Message::Result { outcome, .. } = decoded {
            if let Some(WorkerOutcome::Completed(result)) = outcome.into_public() {
                assert_eq!(result.output.as_bytes(), vec![0; MAX_RESPONSE_BYTES]);
            } else {
                panic!()
            }
        } else {
            panic!()
        }
    }
}
