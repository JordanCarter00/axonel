use plexis_core::ids::WorkflowId;
use plexis_core::{Agent, ExecutionProfile, MemoryRecord, MemoryScope, Task};
use plexis_memory::ScoredMemory;
use plexis_runtime::context::{ContextBudget, ContextBuilder};

#[test]
fn test_context_engine_provenance_memories_and_omission_receipts() {
    let agent = Agent::new(
        "PlannerAgent",
        "lead_planner",
        ExecutionProfile::new("openai", "gpt-4o"),
    );
    let workflow_id = WorkflowId::new();
    let task = Task::new(workflow_id, "Plan microservice architecture")
        .with_description("Draft architecture diagram and service boundary specifications")
        .with_criteria(vec![
            "file:docs/architecture.md".into(),
            "services >= 3".into(),
        ]);

    // 1. Create candidate memories
    let mut memories = Vec::new();
    for i in 1..=8 {
        let mut rec = MemoryRecord::new(
            MemoryScope::Project,
            "architecture_decision",
            format!(
                "Architecture pattern #{} uses event-driven CQRS and Kafka bus.",
                i
            ),
        );
        rec.importance = 0.8;
        memories.push(ScoredMemory {
            record: rec,
            similarity: 0.85,
            importance_score: 0.8,
            recency_score: 0.9,
            total_score: 0.85,
        });
    }

    let budget = ContextBudget {
        max_memories: 3,
        max_artifact_bytes: 150,
        ..Default::default()
    };

    let large_artifact = "X".repeat(500);

    let summary = ContextBuilder::new(&agent, "Design resilient architecture", &task)
        .with_budget(budget)
        .with_system_rule("Always enforce strict schema validation")
        .with_dependency_summary("upstream_rfc", large_artifact)
        .with_failure_evidence("Previous attempt omitted service boundary diagrams")
        .with_recovery_advice(
            "prompt_refinement",
            2,
            "Missing visual component specifications",
            "Generate Mermaid diagrams for every service cluster",
        )
        .with_memories(memories)
        .build();

    // Verify messages formed
    assert_eq!(summary.messages.len(), 2);
    let system_content = summary.messages[0].content.as_deref().unwrap();
    let user_content = summary.messages[1].content.as_deref().unwrap();

    // Verify system rules and agent identity present
    assert!(system_content.contains("PlannerAgent"));
    assert!(system_content.contains("Always enforce strict schema validation"));

    // Verify failure evidence and recovery advice present
    assert!(user_content.contains("Previous attempt omitted service boundary diagrams"));
    assert!(user_content.contains("RECOVERY STRATEGY (v2) - prompt_refinement"));
    assert!(user_content.contains("Generate Mermaid diagrams for every service cluster"));

    // Verify memories were injected up to limit (3 included)
    assert!(user_content.contains("Architecture pattern #1"));
    assert!(user_content.contains("Architecture pattern #3"));
    assert!(!user_content.contains("Architecture pattern #4"));

    // Verify Omission Receipts generated
    assert!(!summary.omission_receipts.is_empty());
    assert!(summary
        .omission_receipts
        .iter()
        .any(|r| r.section == "persistent_memories" && r.count == 5));
    assert!(summary
        .omission_receipts
        .iter()
        .any(|r| r.section.contains("upstream_rfc")));

    // Verify Omission Receipts advisory present in the prompt text
    assert!(user_content.contains("[CONTEXT ADVISORY - OMISSION RECEIPTS]"));
    assert!(user_content.contains("Omitted/Truncated in 'persistent_memories': 5 items"));

    // Verify Provenance itemization
    let prov = &summary.provenance;
    assert!(prov.total_estimated_tokens > 0);
    assert!(prov
        .items
        .iter()
        .any(|item| item.section == "system_identity_and_rules"));
    assert!(prov
        .items
        .iter()
        .any(|item| item.section == "task_contract"));
    assert!(prov
        .items
        .iter()
        .any(|item| item.section == "failure_and_recovery_advice"));
    assert!(prov
        .items
        .iter()
        .any(|item| item.section == "persistent_memory"));
    assert!(prov
        .items
        .iter()
        .any(|item| item.section == "dependency_artifact" && item.truncated));
}
