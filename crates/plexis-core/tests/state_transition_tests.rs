use chrono::{Duration as ChronoDuration, Utc};
use plexis_core::ids::{AgentId, TaskId, WorkflowId};
use plexis_core::lease::Lease;
use plexis_core::session::{Session, SessionState};
use plexis_core::state::{AgentState, CommandState, ExecutionState, TaskState, WorkflowState};
use plexis_core::{ApprovalRecord, ApprovalState};

#[test]
fn test_task_state_exhaustive_transition_matrix() {
    let all_states = [
        TaskState::Backlog,
        TaskState::Ready,
        TaskState::Assigned,
        TaskState::Running,
        TaskState::AwaitingVerification,
        TaskState::Verified,
        TaskState::Blocked,
        TaskState::NeedsHuman,
        TaskState::Paused,
        TaskState::Failed,
        TaskState::Retrying,
        TaskState::Quarantined,
        TaskState::Cancelled,
        TaskState::Discarded,
    ];

    for from in &all_states {
        for to in &all_states {
            let mut state = *from;
            let can = from.can_transition_to(to);
            let res = state.transition_to(*to);

            if can {
                assert!(
                    res.is_ok(),
                    "Expected transition from {:?} to {:?} to succeed",
                    from,
                    to
                );
                assert_eq!(state, *to);
            } else {
                assert!(
                    res.is_err(),
                    "Expected transition from {:?} to {:?} to fail",
                    from,
                    to
                );
            }
        }
    }

    // Terminal invariant: Cancelled and Discarded must never transition to any other state
    for terminal in [TaskState::Cancelled, TaskState::Discarded] {
        assert!(terminal.is_terminal());
        for other in &all_states {
            if other != &terminal {
                assert!(
                    !terminal.can_transition_to(other),
                    "Terminal state {:?} must not allow transition to {:?}",
                    terminal,
                    other
                );
            }
        }
    }

    // Verified can only transition to Retrying or Discarded
    assert!(TaskState::Verified.is_terminal());
    assert!(TaskState::Verified.can_transition_to(&TaskState::Retrying));
    assert!(TaskState::Verified.can_transition_to(&TaskState::Discarded));
    assert!(!TaskState::Verified.can_transition_to(&TaskState::Running));
    assert!(!TaskState::Verified.can_transition_to(&TaskState::Backlog));
}

#[test]
fn test_agent_state_exhaustive_transition_matrix() {
    let all_states = [
        AgentState::Idle,
        AgentState::Busy,
        AgentState::WaitingForInput,
        AgentState::Paused,
        AgentState::Terminated,
        AgentState::Failed,
    ];

    for from in &all_states {
        for to in &all_states {
            let mut state = *from;
            let can = from.can_transition_to(to);
            let res = state.transition_to(*to);

            if can {
                assert!(
                    res.is_ok(),
                    "Expected agent transition from {:?} to {:?} to succeed",
                    from,
                    to
                );
                assert_eq!(state, *to);
            } else {
                assert!(
                    res.is_err(),
                    "Expected agent transition from {:?} to {:?} to fail",
                    from,
                    to
                );
            }
        }
    }

    // Terminated cannot transition to anything
    assert!(!AgentState::Terminated.can_transition_to(&AgentState::Idle));
    assert!(!AgentState::Terminated.can_transition_to(&AgentState::Busy));
}

#[test]
fn test_workflow_state_exhaustive_transition_matrix() {
    let all_states = [
        WorkflowState::Draft,
        WorkflowState::Active,
        WorkflowState::Paused,
        WorkflowState::Completed,
        WorkflowState::Failed,
        WorkflowState::Cancelled,
    ];

    for from in &all_states {
        for to in &all_states {
            let mut state = *from;
            let can = from.can_transition_to(to);
            let res = state.transition_to(*to);

            if can {
                assert!(
                    res.is_ok(),
                    "Expected workflow transition from {:?} to {:?} to succeed",
                    from,
                    to
                );
                assert_eq!(state, *to);
            } else {
                assert!(
                    res.is_err(),
                    "Expected workflow transition from {:?} to {:?} to fail",
                    from,
                    to
                );
            }
        }
    }

    // Terminal workflow states cannot transition
    for terminal in [
        WorkflowState::Completed,
        WorkflowState::Failed,
        WorkflowState::Cancelled,
    ] {
        assert!(!terminal.can_transition_to(&WorkflowState::Active));
        assert!(!terminal.can_transition_to(&WorkflowState::Draft));
    }
}

#[test]
fn test_execution_state_exhaustive_transition_matrix() {
    let all_states = [
        ExecutionState::Pending,
        ExecutionState::Running,
        ExecutionState::Completed,
        ExecutionState::Failed,
        ExecutionState::TimedOut,
        ExecutionState::Cancelled,
    ];

    for from in &all_states {
        for to in &all_states {
            let mut state = *from;
            let can = from.can_transition_to(to);
            let res = state.transition_to(*to);

            if can {
                assert!(
                    res.is_ok(),
                    "Expected execution transition from {:?} to {:?} to succeed",
                    from,
                    to
                );
                assert_eq!(state, *to);
            } else {
                assert!(
                    res.is_err(),
                    "Expected execution transition from {:?} to {:?} to fail",
                    from,
                    to
                );
            }
        }
    }

    // Completed execution cannot reopen
    assert!(!ExecutionState::Completed.can_transition_to(&ExecutionState::Running));
    assert!(!ExecutionState::Completed.can_transition_to(&ExecutionState::Pending));
}

#[test]
fn test_session_state_exhaustive_transition_matrix() {
    let all_states = [
        SessionState::Active,
        SessionState::Paused,
        SessionState::Closed,
    ];

    for from in &all_states {
        for to in &all_states {
            let mut state = *from;
            let can = from.can_transition_to(to);
            let res = state.transition_to(*to);

            if can {
                assert!(
                    res.is_ok(),
                    "Expected session transition from {:?} to {:?} to succeed",
                    from,
                    to
                );
                assert_eq!(state, *to);
            } else {
                assert!(
                    res.is_err(),
                    "Expected session transition from {:?} to {:?} to fail",
                    from,
                    to
                );
            }
        }
    }

    let mut session = Session::new(AgentId::new());
    assert!(session.is_active());
    assert_eq!(session.state, SessionState::Active);

    session.pause().unwrap();
    assert!(!session.is_active());
    assert_eq!(session.state, SessionState::Paused);

    session.resume().unwrap();
    assert!(session.is_active());
    assert_eq!(session.state, SessionState::Active);

    session.close().unwrap();
    assert!(!session.is_active());
    assert!(session.state.is_terminal());

    // Closing a closed session must be idempotent (can transition to self)
    assert!(session.close().is_ok());
    // Resuming a closed session must fail
    assert!(session.resume().is_err());
}

#[test]
fn test_command_state_exhaustive_transition_matrix() {
    let all_states = [
        CommandState::Queued,
        CommandState::Dispatched,
        CommandState::Delivered,
        CommandState::Confirmed,
        CommandState::Failed,
        CommandState::Retrying,
        CommandState::DeadLettered,
        CommandState::Cancelled,
    ];

    for from in &all_states {
        for to in &all_states {
            let mut state = *from;
            let can = from.can_transition_to(to);
            let res = state.transition_to(*to);

            if can {
                assert!(
                    res.is_ok(),
                    "Expected command transition from {:?} to {:?} to succeed",
                    from,
                    to
                );
                assert_eq!(state, *to);
            } else {
                assert!(
                    res.is_err(),
                    "Expected command transition from {:?} to {:?} to fail",
                    from,
                    to
                );
            }
        }
    }

    // Terminal command states
    assert!(!CommandState::Confirmed.can_transition_to(&CommandState::Queued));
    assert!(!CommandState::DeadLettered.can_transition_to(&CommandState::Queued));
    assert!(!CommandState::Cancelled.can_transition_to(&CommandState::Queued));
}

#[test]
fn test_approval_state_and_record_mutations() {
    let mut appr = ApprovalRecord::new(TaskId::new(), WorkflowId::new(), "Deploy to Production");
    assert_eq!(appr.state, ApprovalState::Pending);
    assert!(!appr.state.is_terminal());

    // Approve once
    assert!(appr.approve(Some("Approved by lead".into())).is_ok());
    assert_eq!(appr.state, ApprovalState::Approved);
    assert!(appr.state.is_terminal());
    assert!(appr.decided_at.is_some());

    // Illegal subsequent rejection after already approved
    let reject_err = appr.reject(Some("Changed mind".into()));
    assert!(reject_err.is_err());
    assert_eq!(appr.state, ApprovalState::Approved); // State remains untouched

    // Another record tested with rejection
    let mut appr2 = ApprovalRecord::new(TaskId::new(), WorkflowId::new(), "Format disk");
    assert!(appr2.reject(Some("Denied by security".into())).is_ok());
    assert_eq!(appr2.state, ApprovalState::Rejected);

    // Illegal subsequent approval after already rejected
    let approve_err = appr2.approve(Some("Overridden".into()));
    assert!(approve_err.is_err());
    assert_eq!(appr2.state, ApprovalState::Rejected);
}

#[test]
fn test_lease_fencing_token_validation() {
    let task_id = TaskId::new();
    let agent_id = AgentId::new();
    let lease = Lease::new(task_id, agent_id, ChronoDuration::seconds(10));

    let now = Utc::now();
    // Valid token and unexpired time
    assert!(lease.validate_token(1, now).is_ok());

    // Mismatched generation (stale or future)
    assert!(lease.validate_token(0, now).is_err());
    assert!(lease.validate_token(2, now).is_err());

    // Expired lease
    let future = now + ChronoDuration::seconds(15);
    assert!(lease.is_expired(future));
    assert!(lease.validate_token(1, future).is_err());
}
