import React, { useEffect, useState } from 'react';
import {
  X,
  CheckCircle2,
  AlertTriangle,
  Clock,
  MessageSquare,
  Shield,
  Bot,
  Terminal,
  RefreshCw,
  UserCheck,
  Cpu,
  GitCommit,
} from 'lucide-react';
import {
  Task,
  ExecutionRecord,
  Verification,
  AgentMessage,
  RecoveryRecord,
  Agent,
} from '../types';
import { api } from '../services/api';
import { TerminalView } from './TerminalView';
import { TaskReviewModal } from './TaskReviewModal';

interface TaskDetailDrawerProps {
  taskId: string | null;
  onClose: () => void;
  onSelectTask: (id: string) => void;
  availableAgents: Agent[];
  workspaceId?: string | null;
  onTaskUpdated?: () => void;
}

export const TaskDetailDrawer: React.FC<TaskDetailDrawerProps> = ({
  taskId,
  onClose,
  onSelectTask,
  availableAgents,
  workspaceId,
  onTaskUpdated,
}) => {
  const [dependencies, setDependencies] = useState<{
    task_id: string;
    prerequisites: Task[];
    dependents: Task[];
  } | null>(null);
  const [taskData, setTaskData] = useState<Task | null>(null);
  const [executions, setExecutions] = useState<ExecutionRecord[]>([]);
  const [verifications, setVerifications] = useState<Verification[]>([]);
  const [messages, setMessages] = useState<AgentMessage[]>([]);
  const [recoveries, setRecoveries] = useState<RecoveryRecord[]>([]);
  const [loading, setLoading] = useState(false);
  const [selectedAgentId, setSelectedAgentId] = useState('');
  const [reassigning, setReassigning] = useState(false);
  const [activeTab, setActiveTab] = useState<'overview' | 'terminal' | 'diagnostics'>('overview');
  const [isReviewModalOpen, setIsReviewModalOpen] = useState(false);

  useEffect(() => {
    if (!taskId) return;
    setLoading(true);

    Promise.all([
      api.getTask(taskId).catch(() => null),
      api.getTaskDependencies(taskId).catch(() => null),
      api.getTaskExecutions(taskId).catch(() => []),
      api.getTaskVerifications(taskId).catch(() => []),
      api.getTaskMessages(taskId).catch(() => []),
      api.getTaskRecoveries(taskId).catch(() => []),
    ]).then(([task, deps, execs, verifs, msgs, recovs]) => {
      setTaskData(task);
      setDependencies(deps);
      setExecutions(execs);
      setVerifications(verifs);
      setMessages(msgs);
      setRecoveries(recovs);
      setLoading(false);
    });
  }, [taskId]);

  if (!taskId) return null;

  const handleReassign = async () => {
    if (!selectedAgentId || !taskId) return;
    setReassigning(true);
    try {
      await api.reassignTask(taskId, selectedAgentId);
      if (onTaskUpdated) onTaskUpdated();
    } catch (e) {
      alert(`Reassign failed: ${e}`);
    } finally {
      setReassigning(false);
    }
  };

  const prereqs = dependencies?.prerequisites || [];
  const dependents = dependencies?.dependents || [];

  const currentTask =
    taskData ||
    prereqs.find((p) => p.id === taskId) ||
    dependents.find((d) => d.id === taskId);

  const latestExecution = executions.length > 0 ? executions[executions.length - 1] : null;
  const taskMetadata = (currentTask?.metadata || {}) as Record<string, any>;
  const execMetadata = (latestExecution?.metadata || {}) as Record<string, any>;

  const currentBackend =
    (taskMetadata.backend as string) ||
    (execMetadata.backend as string) ||
    (execMetadata.backend_id as string) ||
    (currentTask?.assigned_agent_id ? 'Internal Agent' : null);

  const isExternalBackend = Boolean(
    currentBackend &&
      currentBackend !== 'internal' &&
      currentBackend !== 'Internal Agent'
  );

  const currentCommitSha =
    (taskMetadata.commit_sha as string) ||
    (execMetadata.commit_sha as string) ||
    null;

  const currentChangedFiles =
    ((taskMetadata.changed_files as string[]) ||
      (execMetadata.changed_files as string[]) ||
      (execMetadata.modified_files as string[]) ||
      []) as string[];

  // Compute "Why is this task in this state?"
  const computeStateExplanation = () => {
    if (!dependencies && !taskData) return 'Evaluating system state...';
    const unfulfilledPrereqs = prereqs.filter((p) => p.state !== 'Verified');

    if (unfulfilledPrereqs.length > 0) {
      return `Waiting on ${unfulfilledPrereqs.length} prerequisite task(s) to complete verification (${unfulfilledPrereqs
        .map((p) => p.objective)
        .join(', ')}).`;
    }

    if (recoveries.length > 0 && recoveries.some((r) => r.status === 'Active')) {
      return `Under active failure recovery strategy: ${
        recoveries[recoveries.length - 1].strategy
      }.`;
    }

    if (verifications.length > 0) {
      const lastVerif = verifications[verifications.length - 1];
      if (lastVerif.passed) {
        return `Independently verified: "${lastVerif.verdict}". Requirements satisfied.`;
      } else {
        return `Verification failed: "${lastVerif.verdict}". Recovery intervention required.`;
      }
    }

    if (executions.length > 0 && executions.some((e) => !e.finished_at && !e.completed_at)) {
      return `Executing in isolated ${isExternalBackend ? `external process [${currentBackend}]` : 'sandbox'} under active agent lease.`;
    }

    return 'Ready for scheduler dispatch. Prerequisites satisfied.';
  };

  return (
    <div className="fixed inset-y-0 right-0 w-full max-w-xl bg-surface border-l border-surface-border shadow-2xl z-50 flex flex-col">
      {/* Drawer Header */}
      <div className="p-4 border-b border-surface-border flex items-center justify-between bg-[#0e1424]">
        <div className="flex items-center space-x-2">
          <Terminal className="w-4 h-4 text-indigo-400" />
          <h2 className="text-sm font-semibold text-slate-200">Task Inspection & Governance</h2>
        </div>
        <div className="flex items-center space-x-2">
          {currentTask && (
            <button
              onClick={() => setIsReviewModalOpen(true)}
              className="px-2.5 py-1 text-xs font-medium bg-indigo-600 hover:bg-indigo-500 text-white rounded transition shadow-sm flex items-center space-x-1"
            >
              <span>Review Surface</span>
            </button>
          )}
          <button
            onClick={onClose}
            className="p-1 rounded text-slate-400 hover:text-slate-200 hover:bg-surface-hover"
          >
            <X className="w-5 h-5" />
          </button>
        </div>
      </div>

      {/* Tabs */}
      <div className="flex items-center space-x-1 px-4 border-b border-surface-border bg-[#0a0d16] text-xs">
        <button
          onClick={() => setActiveTab('overview')}
          className={`px-3 py-2 font-medium border-b-2 transition ${
            activeTab === 'overview'
              ? 'border-indigo-500 text-indigo-300'
              : 'border-transparent text-slate-400 hover:text-slate-200'
          }`}
        >
          Overview
        </button>
        <button
          onClick={() => setActiveTab('terminal')}
          className={`px-3 py-2 font-medium border-b-2 transition ${
            activeTab === 'terminal'
              ? 'border-indigo-500 text-indigo-300'
              : 'border-transparent text-slate-400 hover:text-slate-200'
          }`}
        >
          Live Terminal
        </button>
        <button
          onClick={() => setActiveTab('diagnostics')}
          className={`px-3 py-2 font-medium border-b-2 transition ${
            activeTab === 'diagnostics'
              ? 'border-indigo-500 text-indigo-300'
              : 'border-transparent text-slate-400 hover:text-slate-200'
          }`}
        >
          8 Diagnostics
        </button>
      </div>

      {/* Drawer Body */}
      <div className="flex-1 overflow-y-auto p-4 space-y-6">
        {loading ? (
          <div className="flex items-center justify-center py-12">
            <RefreshCw className="w-5 h-5 animate-spin text-primary-400" />
          </div>
        ) : activeTab === 'terminal' ? (
          <div className="h-[520px]">
            <TerminalView taskId={taskId} isTaskActive={currentTask?.state === 'Running'} />
          </div>
        ) : activeTab === 'diagnostics' ? (
          <div className="space-y-4 text-xs font-sans">
            <div className="p-3 bg-[#121929] border border-surface-border rounded-lg">
              <span className="font-semibold text-indigo-400 block mb-1">1. What is this task trying to do?</span>
              <p className="text-slate-300 leading-relaxed">{currentTask?.description || currentTask?.objective || taskId}</p>
            </div>
            <div className="p-3 bg-[#121929] border border-surface-border rounded-lg">
              <span className="font-semibold text-indigo-400 block mb-1">2. Which files will change?</span>
              <p className="text-slate-300 leading-relaxed font-mono">
                {currentChangedFiles.length > 0
                  ? `Modified files (${currentChangedFiles.length}): ${currentChangedFiles.join(', ')}`
                  : 'Workspace directory scoped modifications.'}
              </p>
            </div>
            <div className="p-3 bg-[#121929] border border-surface-border rounded-lg">
              <span className="font-semibold text-indigo-400 block mb-1">3. What tools or agent backends ran?</span>
              <p className="text-slate-300 leading-relaxed">
                {isExternalBackend
                  ? `Executed via external agent process [${currentBackend}] with sandboxed process-group supervision.`
                  : executions.length > 0
                  ? `${executions.length} internal agent execution attempt(s) recorded with tool invocations.`
                  : 'No tool execution records yet.'}
              </p>
            </div>
            <div className="p-3 bg-[#121929] border border-surface-border rounded-lg">
              <span className="font-semibold text-indigo-400 block mb-1">4. Did tests pass, fail, or not run?</span>
              <p className="text-slate-300 leading-relaxed">
                {verifications.length > 0
                  ? `Verifications recorded: ${verifications[0].verdict} (${verifications[0].passed ? 'PASSED' : 'FAILED'})`
                  : 'Awaiting verification pass.'}
              </p>
            </div>
            <div className="p-3 bg-[#121929] border border-surface-border rounded-lg">
              <span className="font-semibold text-indigo-400 block mb-1">5. Why is human approval needed?</span>
              <p className="text-slate-300 leading-relaxed">
                {currentTask?.state === 'Blocked'
                  ? 'Task is gated on operator sign-off before proceeding.'
                  : 'Governed by autonomous execution policy.'}
              </p>
            </div>
            <div className="p-3 bg-[#121929] border border-surface-border rounded-lg">
              <span className="font-semibold text-indigo-400 block mb-1">6. What command will run if approved?</span>
              <p className="text-slate-300 leading-relaxed">Task assignment execution under runtime scheduler lease.</p>
            </div>
            <div className="p-3 bg-[#121929] border border-surface-border rounded-lg">
              <span className="font-semibold text-indigo-400 block mb-1">7. What changed since the previous attempt?</span>
              <p className="text-slate-300 leading-relaxed">
                {recoveries.length > 0
                  ? `${recoveries.length} recovery attempt(s) performed. Latest strategy: ${recoveries[0].strategy}`
                  : 'Initial execution attempt; no failure mutation applied.'}
              </p>
            </div>
            <div className="p-3 bg-[#121929] border border-surface-border rounded-lg">
              <span className="font-semibold text-indigo-400 block mb-1">8. How does this task fit into the overall plan?</span>
              <p className="text-slate-300 leading-relaxed">
                DAG task with priority {currentTask?.priority ?? 1}. Prerequisites: {prereqs.length}, Dependents: {dependents.length}.
              </p>
            </div>
          </div>
        ) : (
          <>
            {/* Task Info & Objective */}
            <div className="space-y-2">
              <div className="flex items-center justify-between">
                <div className="text-[11px] font-mono text-slate-400">ID: {taskId}</div>
                {isExternalBackend ? (
                  <span className="inline-flex items-center px-2 py-0.5 rounded text-[10px] font-medium bg-cyan-500/10 text-cyan-400 border border-cyan-500/30">
                    <Cpu className="w-3 h-3 mr-1 text-cyan-400" />
                    External Process [{currentBackend}]
                  </span>
                ) : (
                  <span className="inline-flex items-center px-2 py-0.5 rounded text-[10px] font-medium bg-slate-800 text-slate-400 border border-slate-700">
                    <Bot className="w-3 h-3 mr-1 text-slate-400" />
                    Internal Agent
                  </span>
                )}
              </div>
              <h3 className="text-base font-semibold text-slate-100">
                {currentTask?.objective || taskId}
              </h3>
              {currentTask?.description && (
                <p className="text-xs text-slate-400">{currentTask.description}</p>
              )}

              {/* Commit provenance badge if present */}
              {currentCommitSha && (
                <div className="flex items-center space-x-2 pt-1">
                  <span
                    className="inline-flex items-center px-2 py-0.5 rounded text-[11px] font-mono bg-indigo-500/10 text-indigo-400 border border-indigo-500/30"
                    title={`Full Git SHA: ${currentCommitSha}`}
                  >
                    <GitCommit className="w-3 h-3 mr-1 text-indigo-400" />
                    commit: {currentCommitSha.slice(0, 8)}
                  </span>
                  {currentChangedFiles.length > 0 && (
                    <span className="text-[11px] text-slate-400 font-mono">
                      ({currentChangedFiles.length} file{currentChangedFiles.length === 1 ? '' : 's'} modified)
                    </span>
                  )}
                </div>
              )}
            </div>

            {/* State Diagnostic Banner: Why is this task in its state? */}
            <div className="p-3 bg-[#162035] border border-indigo-500/30 rounded-lg">
              <div className="flex items-center space-x-2 mb-1">
                <Clock className="w-3.5 h-3.5 text-indigo-400" />
                <span className="text-xs font-semibold text-indigo-300 uppercase tracking-wider">
                  State Diagnostic
                </span>
              </div>
              <p className="text-xs text-slate-300 font-mono leading-relaxed">
                {computeStateExplanation()}
              </p>
            </div>

            {/* Prerequisites & Dependents */}
            <div className="space-y-3">
              <h4 className="text-xs font-semibold uppercase tracking-wider text-slate-400">
                DAG Dependencies
              </h4>
              <div className="space-y-2">
                <div className="text-xs text-slate-400">
                  Prerequisites ({prereqs.length}):
                </div>
                {prereqs.length === 0 ? (
                  <div className="text-[11px] font-mono text-slate-400 pl-2">None (Root Task)</div>
                ) : (
                  prereqs.map((p) => (
                    <div
                      key={p.id}
                      onClick={() => onSelectTask(p.id)}
                      className="p-2 bg-[#121929] hover:bg-surface-hover border border-surface-border rounded cursor-pointer flex items-center justify-between transition-colors"
                    >
                      <span className="text-xs text-slate-200 truncate max-w-[280px]">
                        {p.objective}
                      </span>
                      <span
                        className={`text-[9px] font-mono uppercase px-1.5 py-0.5 rounded border ${
                          p.state === 'Verified'
                            ? 'bg-emerald-500/10 text-emerald-400 border-emerald-500/30'
                            : 'bg-amber-500/10 text-amber-400 border-amber-500/30'
                        }`}
                      >
                        {p.state}
                      </span>
                    </div>
                  ))
                )}
              </div>

              <div className="space-y-2 mt-3">
                <div className="text-xs text-slate-400">
                  Dependents ({dependents.length}):
                </div>
                {dependents.length === 0 ? (
                  <div className="text-[11px] font-mono text-slate-400 pl-2">None (Terminal Task)</div>
                ) : (
                  dependents.map((d) => (
                    <div
                      key={d.id}
                      onClick={() => onSelectTask(d.id)}
                      className="p-2 bg-[#121929] hover:bg-surface-hover border border-surface-border rounded cursor-pointer flex items-center justify-between transition-colors"
                    >
                      <span className="text-xs text-slate-200 truncate max-w-[280px]">
                        {d.objective}
                      </span>
                      <span className="text-[9px] font-mono uppercase px-1.5 py-0.5 rounded border bg-slate-800 text-slate-400 border-slate-700">
                        {d.state}
                      </span>
                    </div>
                  ))
                )}
              </div>
            </div>

            {/* Agent Assignment & Reassignment */}
            <div className="p-3 bg-[#111726] border border-surface-border rounded-lg space-y-3">
              <div className="flex items-center space-x-2">
                <Bot className="w-3.5 h-3.5 text-purple-400" />
                <span className="text-xs font-semibold text-slate-300 uppercase tracking-wider">
                  Agent Allocation
                </span>
              </div>
              <div className="text-xs text-slate-300">
                Current Agent:{' '}
                <span className="font-mono text-indigo-400">
                  {currentTask?.assigned_agent_id || 'Unassigned'}
                </span>
              </div>
              <div className="flex items-center space-x-2">
                <select
                  value={selectedAgentId}
                  onChange={(e) => setSelectedAgentId(e.target.value)}
                  className="bg-[#0a0d14] border border-surface-border rounded px-2.5 py-1 text-xs text-slate-200 flex-1"
                >
                  <option value="">Select an agent to reassign...</option>
                  {availableAgents.map((a) => (
                    <option key={a.id} value={a.id}>
                      {a.display_name} ({a.role} - {a.state})
                    </option>
                  ))}
                </select>
                <button
                  disabled={!selectedAgentId || reassigning}
                  onClick={handleReassign}
                  className="px-3 py-1 bg-indigo-600 hover:bg-indigo-500 disabled:opacity-50 text-white rounded text-xs font-medium flex items-center space-x-1"
                >
                  <UserCheck className="w-3 h-3" />
                  <span>Reassign</span>
                </button>
              </div>
            </div>

            {/* Independent Verification Results */}
            <div className="space-y-2">
              <div className="flex items-center space-x-2">
                <Shield className="w-3.5 h-3.5 text-emerald-400" />
                <span className="text-xs font-semibold uppercase tracking-wider text-slate-400">
                  Independent Verifications ({verifications.length})
                </span>
              </div>
              {verifications.length === 0 ? (
                <div className="p-3 bg-[#111726] border border-surface-border rounded text-xs text-slate-400 font-mono">
                  No verifications recorded yet.
                </div>
              ) : (
                verifications.map((v) => (
                  <div
                    key={v.id}
                    className={`p-3 rounded border ${
                      v.passed
                        ? 'bg-emerald-950/20 border-emerald-500/30'
                        : 'bg-rose-950/20 border-rose-500/30'
                    } space-y-2`}
                  >
                    <div className="flex items-center justify-between">
                      <div className="flex items-center space-x-2">
                        {v.passed ? (
                          <CheckCircle2 className="w-4 h-4 text-emerald-400" />
                        ) : (
                          <AlertTriangle className="w-4 h-4 text-rose-400" />
                        )}
                        <span className="text-xs font-semibold text-slate-200">{v.verdict}</span>
                      </div>
                      <span className="text-[10px] font-mono text-slate-400">
                        {new Date(v.verified_at).toLocaleTimeString()}
                      </span>
                    </div>
                    {v.evidence && (
                      <div className="text-[11px] font-mono bg-[#0a0d14] p-2 rounded text-slate-300 overflow-x-auto max-h-36">
                        <pre>{JSON.stringify(v.evidence, null, 2)}</pre>
                      </div>
                    )}
                  </div>
                ))
              )}
            </div>

            {/* Executions & Process Supervision */}
            <div className="space-y-2">
              <div className="flex items-center space-x-2">
                <Terminal className="w-3.5 h-3.5 text-sky-400" />
                <span className="text-xs font-semibold uppercase tracking-wider text-slate-400">
                  Executions & Process Supervision ({executions.length})
                </span>
              </div>
              {executions.length === 0 ? (
                <div className="p-3 bg-[#111726] border border-surface-border rounded text-xs text-slate-400 font-mono">
                  No execution records for this task.
                </div>
              ) : (
                executions.map((e) => {
                  const m = (e.metadata || {}) as Record<string, any>;
                  const execBackend = m.backend || m.backend_id;
                  const pid = m.pid;
                  const exitCode = m.exit_code;
                  const duration = m.execution_time_ms;
                  const sha = m.commit_sha;
                  const files = (m.changed_files || m.modified_files || []) as string[];
                  const statusStr = e.state || e.status || 'Unknown';

                  return (
                    <div key={e.id} className="p-3 bg-[#111726] border border-surface-border rounded space-y-2.5">
                      <div className="flex items-center justify-between text-xs">
                        <div className="flex items-center space-x-2">
                          <span className="font-mono font-medium text-slate-300">Attempt #{e.attempt}</span>
                          {execBackend ? (
                            <span className="text-[10px] font-mono px-1.5 py-0.5 rounded bg-cyan-500/10 text-cyan-400 border border-cyan-500/30">
                              External: {execBackend}
                            </span>
                          ) : (
                            <span className="text-[10px] font-mono px-1.5 py-0.5 rounded bg-slate-800 text-slate-400 border border-slate-700">
                              Internal Agent
                            </span>
                          )}
                        </div>
                        <span
                          className={`text-[10px] font-mono uppercase px-2 py-0.5 rounded border ${
                            statusStr.toLowerCase().includes('completed') ||
                            statusStr.toLowerCase().includes('verified')
                              ? 'bg-emerald-500/10 text-emerald-400 border-emerald-500/30'
                              : statusStr.toLowerCase().includes('failed')
                              ? 'bg-rose-500/10 text-rose-400 border-rose-500/30'
                              : 'bg-amber-500/10 text-amber-400 border-amber-500/30'
                          }`}
                        >
                          {statusStr}
                        </span>
                      </div>

                      {/* Process Diagnostics Metadata */}
                      {(pid !== undefined || exitCode !== undefined || duration !== undefined || e.id) && (
                        <div className="flex flex-wrap items-center gap-3 text-[11px] font-mono text-slate-400 bg-[#0a0d14] px-2.5 py-1.5 rounded">
                          {e.id && (
                            <span title={e.id}>
                              Exec ID: <strong className="text-slate-200">{e.id.slice(0, 8)}</strong>
                            </span>
                          )}
                          {pid !== undefined && (
                            <span>
                              PID: <strong className="text-slate-200">{pid}</strong>
                            </span>
                          )}
                          {exitCode !== undefined && (
                            <span>
                              Exit Code:{' '}
                              <strong className={exitCode === 0 ? 'text-emerald-400' : 'text-rose-400'}>
                                {exitCode}
                              </strong>
                            </span>
                          )}
                          {duration !== undefined && (
                            <span>
                              Duration: <strong className="text-slate-200">{duration}ms</strong>
                            </span>
                          )}
                        </div>
                      )}

                      {/* Commit SHA provenance */}
                      {sha && (
                        <div className="p-2 bg-[#0d1322] border border-indigo-500/20 rounded text-xs space-y-1">
                          <div className="flex items-center space-x-1.5 text-indigo-400 font-mono text-[11px]">
                            <GitCommit className="w-3.5 h-3.5" />
                            <span>Git Commit:</span>
                            <span className="font-semibold text-indigo-300 select-all">{sha}</span>
                          </div>
                          {files.length > 0 && (
                            <div className="text-[10px] font-mono text-slate-400 pt-1">
                              <span className="text-slate-500 block mb-0.5">Modified Files:</span>
                              {files.map((f: string) => (
                                <div key={f} className="text-emerald-400 pl-2">
                                  ✓ {f}
                                </div>
                              ))}
                            </div>
                          )}
                        </div>
                      )}

                      {/* Tool Calls if internal */}
                      {e.tool_calls && e.tool_calls.length > 0 && (
                        <div className="space-y-1.5 mt-2">
                          <div className="text-[11px] text-slate-400">Tool Calls:</div>
                          {e.tool_calls.map((tc, idx) => (
                            <div key={idx} className="p-2 bg-[#0a0d14] rounded text-xs font-mono space-y-1">
                              <div className="text-indigo-400 font-semibold">{tc.tool}</div>
                              <div className="text-[10px] text-slate-400">Input: {JSON.stringify(tc.input)}</div>
                              {tc.output !== undefined && (
                                <div className="text-[10px] text-slate-300">Output: {JSON.stringify(tc.output)}</div>
                              )}
                              {tc.error && <div className="text-[10px] text-rose-400">Error: {tc.error}</div>}
                            </div>
                          ))}
                        </div>
                      )}

                      {/* Error banner */}
                      {(e.error || e.error_message) && (
                        <div className="p-2 bg-rose-950/30 border border-rose-500/30 rounded text-xs font-mono text-rose-300">
                          {e.error || e.error_message}
                        </div>
                      )}
                    </div>
                  );
                })
              )}
            </div>

            {/* Failure Recoveries */}
            {recoveries.length > 0 && (
              <div className="space-y-2">
                <div className="flex items-center space-x-2">
                  <AlertTriangle className="w-3.5 h-3.5 text-amber-400" />
                  <span className="text-xs font-semibold uppercase tracking-wider text-slate-400">
                    Recovery Diagnostics ({recoveries.length})
                  </span>
                </div>
                {recoveries.map((r) => (
                  <div key={r.id} className="p-3 bg-amber-950/20 border border-amber-500/30 rounded text-xs font-mono space-y-1">
                    <div className="flex items-center justify-between text-amber-300 font-semibold">
                      <span>Strategy: {r.strategy}</span>
                      <span className="text-[10px] uppercase">{r.status}</span>
                    </div>
                    <div className="text-slate-300">{r.diagnostics}</div>
                  </div>
                ))}
              </div>
            )}

            {/* Task Associated Communication */}
            {messages.length > 0 && (
              <div className="space-y-2">
                <div className="flex items-center space-x-2">
                  <MessageSquare className="w-3.5 h-3.5 text-indigo-400" />
                  <span className="text-xs font-semibold uppercase tracking-wider text-slate-400">
                    Task Messages ({messages.length})
                  </span>
                </div>
                {messages.map((m) => (
                  <div key={m.id} className="p-2.5 bg-[#0a0d14] rounded border border-surface-border font-mono text-xs space-y-1">
                    <div className="flex items-center justify-between text-slate-400 text-[10px]">
                      <span>{m.from_agent} → {m.to_agent}</span>
                      <span>{new Date(m.created_at).toLocaleTimeString()}</span>
                    </div>
                    <div className="text-slate-300">{m.content}</div>
                  </div>
                ))}
              </div>
            )}
          </>
        )}
      </div>

      {currentTask && (
        <TaskReviewModal
          isOpen={isReviewModalOpen}
          onClose={() => setIsReviewModalOpen(false)}
          task={currentTask}
          workspaceId={workspaceId}
          onTaskUpdated={onTaskUpdated}
        />
      )}
    </div>
  );
};
