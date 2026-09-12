# Plexis

**Project:** Plexis  
**Repository:** `roonakyadav/plexis`  
**Local path:** `~/Projects/plexis`  
**Reference implementation:** `~/Projects/amux`  
**Backend:** Rust  
**Interface:** API-first + Web UI  
**Initial deployment:** Single machine  
**Initial domain:** Autonomous software-development workflows  
**Long-term direction:** General-purpose autonomous agent/workflow infrastructure

---

## 1. Product Definition

Plexis is an autonomous agent/workflow system in which a developer gives the system a goal and Plexis plans, decomposes, assigns, executes, coordinates, verifies, recovers, and records the work required to reach that goal.

The initial application is software development, but the underlying runtime must not be hard-coded around coding agents.

The system should support:

- multiple simultaneous agents
- dynamic agent creation
- dynamic task decomposition
- dynamic workflows
- agent-to-agent communication
- persistent execution
- automatic recovery
- independent verification
- human intervention when genuinely necessary
- multiple AI providers
- local models
- tool permissions and sandboxing
- durable state
- complete execution history
- replay/debugging
- persistent memory
- web-based operational control

The fundamental architectural objective is:

> **Build an agent operating system rather than a collection of LLM calls.**

---

# 2. Architectural Principles

## 2.1 Durable state is the source of truth

Live processes are temporary.

The system must be able to reconstruct its logical state after:

- process crashes
- machine restarts
- provider failures
- deployment
- network failures
- agent replacement

The database represents what Plexis believes exists.

Live processes represent what currently exists in reality.

Reconciliation continuously bridges the two.

---

## 2.2 Planning and execution are separate

The planner decides **what should happen**.

The execution runtime decides **how it actually happens**.

The planner must not directly own subprocesses, terminals, provider SDKs, or shell execution.

Conceptually:

```text
Planner
   ↓
Execution Plan
   ↓
Durable Commands
   ↓
Runtime
   ↓
Agent / Tool / Sandbox
   ↓
Events
   ↓
Durable State
```

---

## 2.3 Events are evidence, not the entire state model

Plexis will use conventional durable state plus an immutable audit/event history.

We do not make the entire application depend on event sourcing.

Current state should be cheap to query.

Historical execution should be inspectable and replayable.

---

## 2.4 Everything important must be idempotent

Retries are normal.

Therefore:

- task assignment
- command enqueueing
- agent creation
- tool execution requests
- message delivery
- verification
- workflow advancement
- recovery

must have explicit idempotency semantics wherever duplicate execution could be harmful.

---

## 2.5 Agents are replaceable

An agent is an execution participant, not the permanent owner of intelligence.

A task must survive:

```text
agent crash
agent replacement
provider change
session restart
model change
```

The durable task/workflow state belongs to Plexis.

---

# 3. High-Level Architecture

```text
                         PLEXIS
                            │
                    ┌───────┴────────┐
                    │                 │
              Control Plane     Execution Plane
                    │                 │
          ┌─────────┼─────────┐   ┌───┼─────────────┐
          │         │         │   │   │             │
       Planner   Scheduler  Policy Agent Runtime   Tools
          │         │         │   │   │             │
          └─────────┼─────────┘   │   ├─ Providers
                    │             │   ├─ Sessions
               Task Graph         │   ├─ Sandboxes
                    │             │   └─ Messages
                    └──────┬──────┘
                           │
                    Durable State
                           │
                ┌──────────┼──────────┐
                │          │          │
              State       Events    Memory
                │          │          │
                └──────────┼──────────┘
                           │
                       API Layer
                           │
                       Web UI
```

---

# 4. Core Domain Model

## 4.1 Agent

An **Agent** is a durable logical worker capable of executing tasks.

It contains:

```text
Agent
├── id
├── display_name
├── role
├── provider
├── model
├── workspace
├── capabilities
├── permissions
├── status
├── configuration
├── memory scope
└── current execution
```

Agent identity is independent from the provider.

Plexis must never assume:

```text
agent == Claude
agent == Gemini
agent == Codex
```

Instead:

```text
Agent
  ↓
Execution Profile
  ↓
Provider Adapter
  ↓
Model
```

---

# 5. Task

A **Task** represents a unit of work Plexis wants completed.

A task includes:

```text
Task
├── id
├── objective
├── description
├── parent
├── dependencies
├── children
├── owner/assignment
├── status
├── priority
├── criteria
├── artifacts
├── attempts
├── verification
├── metadata
└── timestamps
```

Tasks belong to Plexis, not to the agent executing them.

An agent may own an execution assignment temporarily.

---

# 6. Workflow

A **Workflow** represents the larger objective containing tasks and relationships.

Workflows are dynamic.

Agents may:

- create tasks
- decompose tasks
- add dependencies
- create subtasks
- propose workflow changes
- request additional agents
- modify execution strategy

But all mutations pass through Plexis policy and durable state.

---

# 7. Task Graph

The task graph is a first-class runtime structure.

Possible relationships:

```text
Task A ──depends-on──> Task B
Task A ──part-of─────> Workflow
Task A ──assigned-to─> Agent
Task A ──produces────> Artifact
Agent ──communicates─> Agent
Task ──blocked-by────> Task
```

The graph must support:

- dependency resolution
- cycle detection
- runnable-task discovery
- dependency-aware scheduling
- decomposition
- prioritization
- graph inspection
- recovery
- visualization

The graph should be reconstructable from durable state.

It must not become a second independent source of truth.

---

# 8. Task Lifecycle

Plexis should use explicit semantic states.

Initial model:

```text
Backlog
   ↓
Ready
   ↓
Assigned
   ↓
Running
   ↓
AwaitingVerification
   ↓
Verified
```

Additional states:

```text
Blocked
NeedsHuman
Paused
Failed
Retrying
Quarantined
Cancelled
Discarded
```

Execution state and task state must remain distinct.

Example:

```text
Task = Running
Agent = WaitingForInput
```

is valid.

Likewise:

```text
Task = AwaitingVerification
Agent = Idle
```

is valid.

This distinction is essential.

---

# 9. Scheduler

The scheduler answers:

> "Given the current durable state, what should execute next?"

Scheduler input:

```text
tasks
agents
dependencies
leases
attempt history
provider availability
permissions
workflow constraints
resource limits
verification status
system state
```

Scheduler output:

```text
AssignmentPlan
├── assignments
├── releases
├── retries
├── stalls
├── escalations
└── coordination actions
```

The scheduler should be deterministic wherever deterministic logic is sufficient.

LLMs should not be responsible for basic invariants such as:

- dependency correctness
- lease correctness
- duplicate prevention
- permission enforcement
- capacity enforcement
- state transition legality

---

# 10. Planner

The planner is the intelligence layer.

Unlike the scheduler, the planner can use an LLM.

It answers:

> "What should we attempt in order to accomplish this objective?"

Planner responsibilities include:

- understanding objectives
- decomposing work
- generating task structures
- proposing strategy
- selecting useful agents
- identifying missing work
- revising plans after failures
- creating recovery strategies

The planner does not directly execute tools.

```text
Goal
 ↓
Planner
 ↓
Plan / graph mutation proposal
 ↓
Validation
 ↓
Durable state
 ↓
Scheduler
```

This separation allows Plexis to improve the planner without destabilizing the runtime.

---

# 11. Agent Runtime

The runtime is responsible for actually executing an agent.

```text
Agent
 ↓
Session
 ↓
Execution Environment
 ↓
Provider
 ↓
Model
```

The runtime owns:

- session lifecycle
- prompt delivery
- cancellation
- state reporting
- provider interaction
- context delivery
- tool requests
- tool responses
- streaming events
- crash detection
- resource accounting

The runtime must expose a provider-neutral interface.

---

# 12. Provider Architecture

Initial providers:

```text
OpenAI
Gemini
Ollama
```

Future providers must be addable without modifying the scheduler.

Conceptually:

```rust
trait Provider {
    fn id(&self) -> ProviderId;
    fn capabilities(&self) -> Capabilities;
    fn models(&self) -> Models;
    fn create_session(...);
    fn send(...);
    fn cancel(...);
    fn usage(...);
}
```

Provider-specific behavior belongs inside adapters.

Plexis core should not contain scattered provider-specific branches.

Provider routing should become a policy-driven subsystem rather than a collection of provider conditionals.

---

# 13. Persistent Sessions

Agents should support long-running sessions.

A session represents the continuity of an agent's execution context.

It may survive:

- multiple turns
- multiple tasks
- process restarts
- provider reconnects
- application restarts

Where provider-native conversation continuity exists, Plexis may preserve it.

But Plexis must never depend exclusively on provider-native history.

Plexis retains its own durable execution/context history.

---

# 14. Command System

The command system is the durable bridge between orchestration and execution.

Example:

```text
ExecuteTask
SendMessage
VerifyTask
ReviewArtifact
ContinueAgent
PauseAgent
ResumeAgent
CancelExecution
CreateAgent
```

A command should have:

```text
id
target
type
payload
state
idempotency_key
created_at
attempts
delivery_policy
preconditions
```

State:

```text
Queued
   ↓
Dispatched
   ↓
Delivered
   ↓
Confirmed
```

Failure/recovery:

```text
Failed
   ↓
Retrying
   ↓
Queued
```

Eventually:

```text
DeadLettered
```

The command queue must guarantee one active command per agent when required by the execution protocol.

---

# 15. Agent-to-Agent Communication

Agent communication is first-class.

An agent should be able to send:

```text
request
question
result
artifact reference
warning
review
rejection
proposal
handoff
```

Messages should be durable.

A message should not depend on one agent being alive at the instant it is generated.

Communication therefore becomes:

```text
Agent A
   ↓
Durable Message
   ↓
Plexis
   ↓
Agent B
```

rather than:

```text
Agent A ─────HTTP/IPC────> Agent B
```

This allows offline recipients, retries, auditing, and replay.

---

# 16. Tool Runtime

Tools are capabilities exposed to agents.

Initial tool categories:

```text
Filesystem
Shell
Git
GitHub
HTTP
Browser
Database
Code Execution
MCP
```

Every invocation should generate a durable execution record containing:

```text
tool
agent
task
arguments
permission decision
start time
end time
result
failure
resource usage
```

Tool results may be large and should have bounded storage/transport policies.

---

# 17. Permission System

Permissions are capability-based.

Examples:

```text
filesystem.read
filesystem.write
shell.execute
git.commit
git.push
network.http
browser.control
github.pull_request
deployment.execute
```

Permissions may be granted at:

```text
system
workflow
agent
task
tool
execution
```

A dangerous tool must never become available simply because an agent requested it.

Authorization occurs before execution.

---

# 18. Sandboxing

Sandboxing exists in v1.

The architecture should support progressively stronger isolation:

```text
Local restricted process
        ↓
Container
        ↓
Isolated container
        ↓
Remote sandbox
```

The initial implementation should optimize for one-machine operation.

The abstraction must not assume that the agent can access the host freely.

---

# 19. Memory

Plexis requires persistent memory independent of individual model sessions.

Memory should have multiple scopes:

```text
System
User
Project
Workflow
Task
Agent
Session
Artifact
```

Memory records should support:

```text
content
scope
metadata
created_at
updated_at
source
confidence
embedding
relationships
```

Vector retrieval becomes the primary semantic retrieval mechanism.

Structured filtering remains necessary.

The memory architecture should therefore combine:

```text
vector search
+
metadata filtering
+
graph relationships
+
recency
+
importance
```

rather than relying exclusively on embeddings.

---

# 20. Context Engine

The context engine builds the information supplied to an agent for a specific execution.

Potential sources:

```text
system rules
agent profile
workflow objective
task
dependencies
previous attempts
messages
artifacts
memory
verification evidence
recent execution history
```

The context engine must:

- respect permissions
- respect visibility
- prioritize relevant information
- enforce context budgets
- track omissions
- avoid unnecessary duplication
- preserve deterministic provenance

A context snapshot should be identifiable and reproducible.

---

# 21. Verification

Plexis distinguishes:

```text
Agent says "done"
```

from:

```text
Plexis has verified the result.
```

Verification is independent.

Possible verifier types:

```text
Unit tests
Integration tests
Build
Type checking
Lint
Static analysis
HTTP checks
Browser tests
Filesystem assertions
Command assertions
Human approval
Independent AI review
Custom validators
```

A verifier should return:

```text
verdict
evidence
duration
checks
failure reason
```

Verification evidence becomes durable.

A task cannot become final merely because an agent claims success.

---

# 22. Verification Independence

For important tasks:

```text
Producer Agent
      ↓
Artifact
      ↓
Independent Verifier
      ↓
Evidence
      ↓
Task state
```

The verifier should not blindly reuse the producer's reasoning.

This is a central Plexis reliability mechanism.

---

# 23. Failure Recovery

Failures are expected states.

Plexis models:

```text
Agent crash
Timeout
Provider failure
Rate limit
Context exhaustion
Tool failure
Invalid result
Verification failure
Dependency failure
Resource exhaustion
Network failure
Sandbox failure
```

Recovery should combine:

```text
retry
restart
resume
re-plan
decompose
change agent
change provider
change model
change strategy
escalate to human
quarantine
```

The recovery system should use previous failure evidence so that:

```text
retry
```

does not simply repeat the same mistake indefinitely.

---

# 24. Adaptive Decomposition

An agent may discover that a task is too large or ambiguous.

It can propose:

```text
Parent Task
├── Child A
├── Child B
├── Child C
└── Child D
```

Plexis validates the proposal and modifies the durable task graph.

There should be protection against infinite recursive decomposition.

Instead of hardcoding an arbitrary small global depth, the system should eventually use multiple safeguards:

```text
progress
budget
graph size
repeated decomposition
failure history
resource limits
```

---

# 25. Human Intervention

Humans should be an escalation mechanism, not the default scheduler.

Plexis should interrupt when:

```text
critical authorization required
destructive action needs approval
system cannot distinguish between materially different choices
repeated recovery failed
policy requires human decision
```

Humans should be able to:

- approve
- reject
- pause
- resume
- cancel
- reassign
- edit tasks
- alter priorities
- override decisions
- inspect execution
- inspect evidence
- communicate with agents

---

# 26. Leases and Concurrency

Plexis must prevent two agents from unintentionally executing the same work.

Assignments use leases.

A lease contains:

```text
task
agent
generation
acquired_at
expires_at
```

Expired work can be reclaimed.

Generation/idempotency semantics prevent stale agents from mutating newer assignments.

---

# 27. Resource Limits

Plexis should treat resources as first-class constraints.

Examples:

```text
token budget
time budget
tool-call budget
cost budget
parallel-agent limit
workflow budget
provider quota
memory budget
sandbox resource limit
```

Resource enforcement belongs below the planner so an LLM cannot bypass system limits.

---

# 28. Provider Fleet Management

Provider availability is separate from individual model routing.

Example:

```text
OpenAI quota exhausted
        ↓
OpenAI agents parked
        ↓
Plexis may redistribute work
        ↓
eligible alternative provider
```

The system must never invent quota reset information.

Provider state should explicitly distinguish:

```text
Available
Exhausted
Unknown
```

---

# 29. Observability

Every meaningful execution should be inspectable.

Plexis should expose:

```text
Workflow timeline
Task timeline
Agent lifecycle
Command lifecycle
Tool executions
Provider usage
Token usage
Cost
Latency
Failures
Retries
Verification
Messages
Memory access
Policy decisions
```

The UI should allow a developer to answer:

> "Why is this agent doing this right now?"

without reading raw logs.

---

# 30. Audit Trail

Important mutations produce immutable records.

Examples:

```text
task_created
task_assigned
task_started
agent_created
command_dispatched
tool_called
permission_granted
permission_denied
message_sent
verification_started
verification_completed
task_retried
task_replanned
human_approval_requested
human_approval_received
```

The audit trail exists independently of current state.

---

# 31. Replay / Debugging

Plexis should be able to reconstruct an execution timeline.

The goal is not necessarily complete deterministic replay of every LLM token.

Instead:

```text
What state existed?
What decision was made?
What inputs were available?
What command was issued?
What happened?
What evidence was produced?
Why did state change?
```

This creates a practical debugging system.

---

# 32. Reconciliation

Startup and runtime reconciliation compares:

```text
Durable state
        vs
Live runtime
```

Examples:

```text
DB says agent running
but process disappeared
```

or:

```text
process exists
but Plexis has no corresponding live session
```

Plexis should detect these situations and recover conservatively.

Uncertain observations should not trigger destructive automatic actions.

---

# 33. Persistence Model

Initial deployment:

```text
Single-machine
```

The persistence abstraction should nevertheless isolate storage from domain logic.

Logical layers:

```text
Domain
 ↓
Repository / Store
 ↓
Database
```

The initial database choice should prioritize:

- transactional correctness
- local development
- reliability
- easy backup
- concurrent reads
- straightforward migrations

The implementation should avoid making the domain layer database-specific.

---

# 34. API

The API becomes the external control boundary.

Core API domains:

```text
/workflows
/tasks
/agents
/sessions
/messages
/tools
/artifacts
/verifications
/memory
/events
/providers
/policies
/system
```

The web UI talks to the API.

The UI must not directly manipulate database state.

---

# 35. Web UI

The initial UI should prioritize operational visibility rather than visual decoration.

Primary surfaces:

```text
Dashboard
Workflow view
Task graph
Agent grid
Agent detail
Live execution
Messages
Tool activity
Verification
Artifacts
Memory
Failures
Timeline
System health
```

A developer should be able to understand the entire running system from the UI.

---

# 36. Repository Structure

Initial monorepo:

```text
plexis/
├── crates/
│   ├── plexis-core/
│   ├── plexis-runtime/
│   ├── plexis-server/
│   ├── plexis-provider/
│   ├── plexis-tools/
│   ├── plexis-storage/
│   ├── plexis-memory/
│   ├── plexis-verification/
│   └── ...
│
├── web/
│
├── migrations/
│
├── tests/
│
├── examples/
│
├── docs/
│
├── scripts/
│
├── Cargo.toml
├── README.md
├── ARCHITECTURE.md
└── AGENTS.md
```

The exact crate split is allowed to evolve.

Do not create crates merely for visual organization.

A package boundary must correspond to a meaningful architectural boundary.

---

# 37. Core Dependency Direction

The intended dependency direction is:

```text
                   ┌───────────────┐
                   │   Web / API   │
                   └───────┬───────┘
                           │
                   ┌───────▼───────┐
                   │    Runtime    │
                   └───────┬───────┘
                           │
        ┌──────────────────┼──────────────────┐
        │                  │                  │
        ▼                  ▼                  ▼
    Scheduler          Execution          Verification
        │                  │                  │
        └──────────────────┼──────────────────┘
                           ▼
                         Core
                           │
                           ▼
                       Storage
```

Core domain logic must not depend on:

```text
HTTP
UI
database implementation
specific provider
terminal
cloud platform
```

---

# 38. What Plexis Inherits Conceptually From Amux

The following ideas have demonstrated architectural value and should influence Plexis:

```text
Pure planning
Durable command queue
Explicit state transitions
Leases
Separate task and execution state
Worker/session separation
Provider abstraction
Structured agent protocol
Context snapshots
Independent verification
Policy enforcement
Execution budgets
Provider fleet state
Startup reconciliation
Durable audit history
Task graph
```

These are architectural concepts, not code to copy.

---

# 39. What Plexis Must Not Blindly Reproduce

Plexis should not inherit Amux's historical complexity merely because it exists.

Particular caution areas:

```text
legacy terminal-driven control paths
parallel old/new orchestration mechanisms
compatibility shims
provider-specific logic leaking into server code
transitional routing paths
legacy API parity layers
historical naming
dead abstractions
duplicate state/control paths
```

The Plexis implementation should have one canonical mechanism wherever possible.

For example:

```text
one command system
one canonical task transition system
one agent protocol
one provider abstraction
one durable state authority
one verification pipeline
```

Compatibility layers should exist only when genuinely required.

---

# 40. Major Plexis Improvements

Plexis should pursue improvements in these areas:

## Architectural clarity

Reduce historical layering and eliminate competing mechanisms.

## Agent abstraction

Make agents, sessions, tools, and providers cleanly independent.

## Multi-agent coordination

Treat communication and shared execution as first-class primitives.

## Recovery

Make failure evidence and adaptive recovery central rather than secondary.

## Memory

Combine structured state with semantic/vector retrieval.

## Security

Design sandboxing and capabilities into the runtime from v1.

## Verification

Treat independent verification as a fundamental execution stage.

## Developer experience

Make the entire system inspectable and understandable through the UI.

## Extensibility

Allow new providers, tools, verifiers, execution environments, and planner strategies without modifying core domain logic.

---

# 41. v1 Boundary

Plexis v1 is:

```text
one machine
+
one project
+
multiple agents
+
dynamic tasks
+
durable execution
+
agent communication
+
tool execution
+
sandboxing
+
persistent memory
+
verification
+
automatic recovery
+
web UI
```

Explicitly deferred:

```text
cloud deployment
large-scale distributed scheduling
Kubernetes orchestration
multi-machine execution
enterprise identity systems
multi-region infrastructure
```

The abstractions should not prevent these later capabilities.

---

# 42. Implementation Strategy

Implementation proceeds from the inside outward.

```text
Phase 1
Core domain
State machine
Identifiers
Task graph
Events
Policies
Limits

        ↓

Phase 2
Persistence
Migrations
Repositories
Transactions
Audit records

        ↓

Phase 3
Agent runtime
Sessions
Provider interface
Command system
Execution lifecycle

        ↓

Phase 4
Tools
Permissions
Sandbox
Artifacts

        ↓

Phase 5
Planner
Scheduler
Decomposition
Recovery
Multi-agent communication

        ↓

Phase 6
Memory
Context
Retrieval

        ↓

Phase 7
Verification

        ↓

Phase 8
API

        ↓

Phase 9
Web UI

        ↓

Phase 10
Benchmarks
Hardening
Failure testing
Packaging
```

---

# 43. The First Vertical Slice

We should not build the entire platform before proving execution.

The first meaningful Plexis milestone should be:

```text
Developer creates workflow
        ↓
Plexis creates task
        ↓
Planner determines next work
        ↓
Scheduler assigns agent
        ↓
Agent starts session
        ↓
Agent executes tools
        ↓
Agent reports completion
        ↓
Independent verifier runs
        ↓
Task becomes Verified
        ↓
Full history visible in UI
```

Everything else should progressively make this path more powerful and reliable.

---

# 44. Definition of Architectural Success

Plexis v1 is architecturally successful when the system can:

1. accept a meaningful software-development objective;
2. dynamically create and modify its task graph;
3. run multiple agents concurrently;
4. communicate between agents;
5. execute real tools inside controlled environments;
6. survive agent/process/provider failures;
7. retry using failure evidence rather than blindly repeating;
8. independently verify completed work;
9. preserve state across restart;
10. expose the entire execution through the web interface.

The repository should simultaneously demonstrate that the author understands:

```text
AI agents
+
runtime systems
+
distributed-systems principles
+
state machines
+
security
+
verification
+
developer infrastructure
```

---

# 45. Engineering Rule for Antigravity

Antigravity is the implementation engine.

It may inspect:

```text
~/Projects/amux
~/Projects/plexis
```

but Plexis code must be authored as an independent implementation.

The process is:

```text
Understand Amux
      ↓
Extract principle
      ↓
Design Plexis abstraction
      ↓
Implement Plexis
      ↓
Test Plexis independently
```

It must not use:

```text
copy
rename
mass transplant
historical Amux comments
Amux-specific identifiers
Amux-specific compatibility assumptions
```

The resulting repository must read as though Plexis was designed and built independently.

---

# 46. Source of Truth Hierarchy

When making implementation decisions:

```text
1. Plexis architecture specification
2. Plexis domain invariants
3. Plexis tests
4. Existing Plexis implementation
5. Amux reference implementation
6. Provider/platform constraints
```

Amux is a **reference**, never the specification.

---

# 47. Immediate Next Implementation Step

The first implementation task is **repository foundation**, not the scheduler.

The initial Plexis repository should establish:

```text
Cargo workspace
core crate
storage crate
runtime crate
server crate
initial migrations
domain identifiers
basic event model
basic task model
basic agent model
basic state machine
tests
README
ARCHITECTURE.md
AGENTS.md
```

Then build the first durable end-to-end execution slice.

This gives Antigravity a stable architectural boundary before large amounts of code accumulate.