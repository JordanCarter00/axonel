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

interface TaskDetailDrawerProps {
  taskId: string | null;
  onClose: () => void;
  onSelectTask: (id: string) => void;
  availableAgents: Agent[];
  onTaskUpdated?: () => void;
}

export const TaskDetailDrawer: React.FC<TaskDetailDrawerProps> = ({
  taskId,
  onClose,
  onSelectTask,
  availableAgents,
  onTaskUpdated,
}) => {
  const [dependencies, setDependencies] = useState<{
    task_id: string;
    prerequisites: Task[];
    dependents: Task[];
  } | null>(null);
  const [executions, setExecutions] = useState<ExecutionRecord[]>([]);
  const [verifications, setVerifications] = useState<Verification[]>([]);
  const [messages, setMessages] = useState<AgentMessage[]>([]);
  const [recoveries, setRecoveries] = useState<RecoveryRecord[]>([]);
  const [loading, setLoading] = useState(false);
  const [selectedAgentId, setSelectedAgentId] = useState('');
  const [reassigning, setReassigning] = useState(false);

  useEffect(() => {
    if (!taskId) return;
    setLoading(true);

    Promise.all([
      api.getTaskDependencies(taskId).catch(() => null),
      api.getTaskExecutions(taskId).catch(() => []),
      api.getTaskVerifications(taskId).catch(() => []),
      api.getTaskMessages(taskId).catch(() => []),
      api.getTaskRecoveries(taskId).catch(() => []),
    ]).then(([deps, execs, verifs, msgs, recovs]) => {
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

  const currentTask =
    dependencies?.prerequisites.find((p) => p.id === taskId) ||
    dependencies?.dependents.find((d) => d.id === taskId);

  // Compute "Why is this task in this state?"
  const computeStateExplanation = () => {
    if (!dependencies) return 'Evaluating system state...';
    const unfulfilledPrereqs = dependencies.prerequisites.filter((p) => p.state !== 'Verified');

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

    if (executions.length > 0 && executions.some((e) => !e.finished_at)) {
      return `Executing in isolated sandbox under active agent lease.`;
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
        <button
          onClick={onClose}
          className="p-1 rounded text-slate-400 hover:text-slate-200 hover:bg-surface-hover"
        >
          <X className="w-5 h-5" />
        </button>
      </div>

      {/* Drawer Body */}
      <div className="flex-1 overflow-y-auto p-4 space-y-6">
        {loading ? (
          <div className="flex items-center justify-center py-12">
            <RefreshCw className="w-5 h-5 animate-spin text-primary-400" />
          </div>
        ) : (
          <>
            {/* Task Info & Objective */}
            <div className="space-y-2">
              <div className="text-[11px] font-mono text-slate-400">ID: {taskId}</div>
              <h3 className="text-base font-semibold text-slate-100">
                {currentTask?.objective || taskId}
              </h3>
              {currentTask?.description && (
                <p className="text-xs text-slate-400">{currentTask.description}</p>
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
                  Prerequisites ({dependencies?.prerequisites.length || 0}):
                </div>
                {dependencies?.prerequisites.length === 0 ? (
                  <div className="text-[11px] font-mono text-slate-400 pl-2">None (Root Task)</div>
                ) : (
                  dependencies?.prerequisites.map((p) => (
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
                  Dependents ({dependencies?.dependents.length || 0}):
                </div>
                {dependencies?.dependents.length === 0 ? (
                  <div className="text-[11px] font-mono text-slate-400 pl-2">None (Terminal Task)</div>
                ) : (
                  dependencies?.dependents.map((d) => (
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

            {/* Tool Executions & Secret Redaction */}
            <div className="space-y-2">
              <div className="flex items-center space-x-2">
                <Terminal className="w-3.5 h-3.5 text-sky-400" />
                <span className="text-xs font-semibold uppercase tracking-wider text-slate-400">
                  Tool Executions & Sandboxing
                </span>
              </div>
              {executions.length === 0 ? (
                <div className="p-3 bg-[#111726] border border-surface-border rounded text-xs text-slate-400 font-mono">
                  No execution records for this task.
                </div>
              ) : (
                executions.map((e) => (
                  <div key={e.id} className="p-3 bg-[#111726] border border-surface-border rounded space-y-2">
                    <div className="flex items-center justify-between text-xs">
                      <span className="font-mono text-slate-300">Attempt #{e.attempt}</span>
                      <span className="text-[10px] font-mono uppercase text-slate-400">{e.status}</span>
                    </div>
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
                    {e.error && (
                      <div className="p-2 bg-rose-950/30 border border-rose-500/30 rounded text-xs font-mono text-rose-300">
                        {e.error}
                      </div>
                    )}
                  </div>
                ))
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
    </div>
  );
};
