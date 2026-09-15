import React, { useEffect, useState } from 'react';
import {
  Workflow,
  WorkflowGraph,
  Task,
  AgentMessage,
  RecoveryRecord,
  Verification,
  Agent,
} from '../types';
import { api } from '../services/api';
import { TaskGraphView } from './TaskGraphView';
import { TaskDetailDrawer } from './TaskDetailDrawer';
import { DiffViewer } from './DiffViewer';
import {
  Play,
  Pause,
  RotateCcw,
  Ban,
  ArrowLeft,
  Share2,
  List,
  MessageSquare,
  ShieldCheck,
  AlertTriangle,
  RefreshCw,
  GitPullRequest,
} from 'lucide-react';
import { GitDiffResponse } from '../types';

interface WorkflowDetailViewProps {
  workflowId: string;
  onBack: () => void;
  availableAgents: Agent[];
  workspaceId?: string | null;
}

type WorkflowSubTab = 'graph' | 'tasks' | 'diff' | 'messages' | 'recoveries' | 'verifications';

export const WorkflowDetailView: React.FC<WorkflowDetailViewProps> = ({
  workflowId,
  onBack,
  availableAgents,
  workspaceId,
}) => {
  const [workflow, setWorkflow] = useState<Workflow | null>(null);
  const [graph, setGraph] = useState<WorkflowGraph | null>(null);
  const [tasks, setTasks] = useState<Task[]>([]);
  const [messages, setMessages] = useState<AgentMessage[]>([]);
  const [recoveries, setRecoveries] = useState<RecoveryRecord[]>([]);
  const [verifications, setVerifications] = useState<Verification[]>([]);
  const [selectedTaskId, setSelectedTaskId] = useState<string | null>(null);
  const [activeSubTab, setActiveSubTab] = useState<WorkflowSubTab>('graph');
  const [loading, setLoading] = useState(true);
  const [actionLoading, setActionLoading] = useState(false);
  const [diffData, setDiffData] = useState<GitDiffResponse | null>(null);
  const [loadingDiff, setLoadingDiff] = useState(false);

  const effectiveWorkspaceId = workflow?.workspace_id || workspaceId;

  const loadDiff = async () => {
    if (!effectiveWorkspaceId) return;
    setLoadingDiff(true);
    try {
      const d = await api.getGitDiff(effectiveWorkspaceId);
      setDiffData(d);
    } catch (err) {
      console.error('Failed to load git diff:', err);
    } finally {
      setLoadingDiff(false);
    }
  };

  useEffect(() => {
    if (activeSubTab === 'diff') {
      loadDiff();
    }
  }, [activeSubTab, effectiveWorkspaceId]);

  const handleCommit = async (message: string) => {
    if (!effectiveWorkspaceId) return;
    await api.commitGit(effectiveWorkspaceId, message);
    await loadDiff();
  };

  const loadData = async () => {
    try {
      const [w, g, t, m, r, v] = await Promise.all([
        api.getWorkflow(workflowId),
        api.getWorkflowGraph(workflowId),
        api.getWorkflowTasks(workflowId),
        api.getWorkflowMessages(workflowId).catch(() => []),
        api.getWorkflowRecoveries(workflowId).catch(() => []),
        api.getWorkflowVerifications(workflowId).catch(() => []),
      ]);
      setWorkflow(w);
      setGraph(g);
      setTasks(t);
      setMessages(m);
      setRecoveries(r);
      setVerifications(v);
    } catch (err) {
      console.error('Failed to load workflow detail:', err);
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    loadData();
    const interval = setInterval(loadData, 3000);
    return () => clearInterval(interval);
  }, [workflowId]);

  const handlePlan = async () => {
    setActionLoading(true);
    try {
      await api.planWorkflow(workflowId);
      await loadData();
    } catch (e) {
      alert(`Planning failed: ${e}`);
    } finally {
      setActionLoading(false);
    }
  };

  const handleStart = async () => {
    setActionLoading(true);
    try {
      await api.startWorkflow(workflowId);
      await loadData();
    } catch (e) {
      alert(`Start failed: ${e}`);
    } finally {
      setActionLoading(false);
    }
  };

  const handlePause = async () => {
    setActionLoading(true);
    try {
      await api.pauseWorkflow(workflowId);
      await loadData();
    } catch (e) {
      alert(`Pause failed: ${e}`);
    } finally {
      setActionLoading(false);
    }
  };

  const handleResume = async () => {
    setActionLoading(true);
    try {
      await api.resumeWorkflow(workflowId);
      await loadData();
    } catch (e) {
      alert(`Resume failed: ${e}`);
    } finally {
      setActionLoading(false);
    }
  };

  const handleCancel = async () => {
    if (!confirm('Are you sure you want to cancel this workflow?')) return;
    setActionLoading(true);
    try {
      await api.cancelWorkflow(workflowId);
      await loadData();
    } catch (e) {
      alert(`Cancel failed: ${e}`);
    } finally {
      setActionLoading(false);
    }
  };

  if (loading && !workflow) {
    return (
      <div className="flex items-center justify-center min-h-[60vh]">
        <div className="flex flex-col items-center space-y-3">
          <RefreshCw className="w-6 h-6 animate-spin text-primary-400" />
          <span className="text-sm font-mono text-slate-400">Loading workflow...</span>
        </div>
      </div>
    );
  }

  if (!workflow) {
    return (
      <div className="p-8 text-center">
        <p className="text-slate-400">Workflow not found.</p>
        <button onClick={onBack} className="mt-4 px-3 py-1.5 bg-surface-border rounded text-xs">
          Back to Workflows
        </button>
      </div>
    );
  }

  const getStatusBadge = (state: string) => {
    switch (state.toLowerCase()) {
      case 'executing':
        return 'bg-sky-500/10 text-sky-400 border-sky-500/30 animate-pulse';
      case 'completed':
        return 'bg-emerald-500/10 text-emerald-400 border-emerald-500/30';
      case 'planned':
        return 'bg-indigo-500/10 text-indigo-400 border-indigo-500/30';
      case 'paused':
        return 'bg-amber-500/10 text-amber-400 border-amber-500/30';
      case 'failed':
        return 'bg-rose-500/10 text-rose-400 border-rose-500/30';
      default:
        return 'bg-slate-800 text-slate-400 border-slate-700';
    }
  };

  return (
    <div className="space-y-6 pb-12">
      {/* Workflow Header & Controls */}
      <div className="bg-surface border border-surface-border rounded-lg p-5">
        <div className="flex flex-col md:flex-row md:items-center justify-between gap-4">
          <div className="space-y-1.5">
            <div className="flex items-center space-x-3">
              <button
                onClick={onBack}
                className="p-1 rounded text-slate-400 hover:text-slate-200 hover:bg-surface-hover"
              >
                <ArrowLeft className="w-4 h-4" />
              </button>
              <h1 className="text-lg font-bold text-slate-100">{workflow.name || workflow.title}</h1>
              <span
                className={`text-xs font-mono uppercase px-2.5 py-0.5 rounded border ${getStatusBadge(
                  workflow.state
                )}`}
              >
                {workflow.state}
              </span>
            </div>
            <p className="text-xs text-slate-300 ml-7">{workflow.description || workflow.objective}</p>
            <div className="text-[11px] font-mono text-slate-400 ml-7">
              ID: {workflow.id} • Created: {new Date(workflow.created_at).toLocaleString()}
            </div>
          </div>

          {/* Action Toolbar */}
          <div className="flex items-center space-x-2">
            {workflow.state === 'Draft' && (
              <button
                disabled={actionLoading}
                onClick={handlePlan}
                className="flex items-center space-x-1.5 px-3 py-1.5 bg-indigo-600 hover:bg-indigo-500 text-white rounded text-xs font-medium transition-colors"
              >
                <GitPullRequest className="w-3.5 h-3.5" />
                <span>Autonomous Plan</span>
              </button>
            )}

            {(workflow.state === 'Planned' || workflow.state === 'Draft') && (
              <button
                disabled={actionLoading}
                onClick={handleStart}
                className="flex items-center space-x-1.5 px-3 py-1.5 bg-sky-600 hover:bg-sky-500 text-white rounded text-xs font-medium transition-colors"
              >
                <Play className="w-3.5 h-3.5" />
                <span>Start Execution</span>
              </button>
            )}

            {workflow.state === 'Executing' && (
              <button
                disabled={actionLoading}
                onClick={handlePause}
                className="flex items-center space-x-1.5 px-3 py-1.5 bg-amber-600 hover:bg-amber-500 text-white rounded text-xs font-medium transition-colors"
              >
                <Pause className="w-3.5 h-3.5" />
                <span>Pause</span>
              </button>
            )}

            {workflow.state === 'Paused' && (
              <button
                disabled={actionLoading}
                onClick={handleResume}
                className="flex items-center space-x-1.5 px-3 py-1.5 bg-emerald-600 hover:bg-emerald-500 text-white rounded text-xs font-medium transition-colors"
              >
                <Play className="w-3.5 h-3.5" />
                <span>Resume</span>
              </button>
            )}

            {workflow.state !== 'Completed' && workflow.state !== 'Cancelled' && (
              <button
                disabled={actionLoading}
                onClick={handleCancel}
                className="flex items-center space-x-1.5 px-3 py-1.5 bg-surface-border hover:bg-rose-950/40 text-slate-400 hover:text-rose-400 border border-surface-border rounded text-xs font-medium transition-colors"
              >
                <Ban className="w-3.5 h-3.5" />
                <span>Cancel</span>
              </button>
            )}

            <button
              onClick={loadData}
              className="p-1.5 text-slate-400 hover:text-slate-200 hover:bg-surface-hover rounded border border-surface-border"
              title="Refresh workflow data"
            >
              <RotateCcw className="w-3.5 h-3.5" />
            </button>
          </div>
        </div>

        {/* Sub Navigation */}
        <div className="flex items-center space-x-1 border-t border-surface-border mt-5 pt-3">
          <button
            onClick={() => setActiveSubTab('graph')}
            className={`flex items-center space-x-1.5 px-3 py-1.5 rounded text-xs font-medium transition-colors ${
              activeSubTab === 'graph'
                ? 'bg-primary-600/20 text-indigo-300 border border-primary-500/30'
                : 'text-slate-400 hover:text-slate-200'
            }`}
          >
            <Share2 className="w-3.5 h-3.5" />
            <span>Interactive DAG</span>
          </button>

          <button
            onClick={() => setActiveSubTab('tasks')}
            className={`flex items-center space-x-1.5 px-3 py-1.5 rounded text-xs font-medium transition-colors ${
              activeSubTab === 'tasks'
                ? 'bg-primary-600/20 text-indigo-300 border border-primary-500/30'
                : 'text-slate-400 hover:text-slate-200'
            }`}
          >
            <List className="w-3.5 h-3.5" />
            <span>Tasks ({tasks.length})</span>
          </button>

          <button
            onClick={() => setActiveSubTab('diff')}
            className={`flex items-center space-x-1.5 px-3 py-1.5 rounded text-xs font-medium transition-colors ${
              activeSubTab === 'diff'
                ? 'bg-primary-600/20 text-indigo-300 border border-primary-500/30'
                : 'text-slate-400 hover:text-slate-200'
            }`}
          >
            <GitPullRequest className="w-3.5 h-3.5" />
            <span>Workspace Diff</span>
          </button>

          <button
            onClick={() => setActiveSubTab('messages')}
            className={`flex items-center space-x-1.5 px-3 py-1.5 rounded text-xs font-medium transition-colors ${
              activeSubTab === 'messages'
                ? 'bg-primary-600/20 text-indigo-300 border border-primary-500/30'
                : 'text-slate-400 hover:text-slate-200'
            }`}
          >
            <MessageSquare className="w-3.5 h-3.5" />
            <span>Agent Messages ({messages.length})</span>
          </button>

          <button
            onClick={() => setActiveSubTab('verifications')}
            className={`flex items-center space-x-1.5 px-3 py-1.5 rounded text-xs font-medium transition-colors ${
              activeSubTab === 'verifications'
                ? 'bg-primary-600/20 text-indigo-300 border border-primary-500/30'
                : 'text-slate-400 hover:text-slate-200'
            }`}
          >
            <ShieldCheck className="w-3.5 h-3.5" />
            <span>Verifications ({verifications.length})</span>
          </button>

          <button
            onClick={() => setActiveSubTab('recoveries')}
            className={`flex items-center space-x-1.5 px-3 py-1.5 rounded text-xs font-medium transition-colors ${
              activeSubTab === 'recoveries'
                ? 'bg-primary-600/20 text-indigo-300 border border-primary-500/30'
                : 'text-slate-400 hover:text-slate-200'
            }`}
          >
            <AlertTriangle className="w-3.5 h-3.5" />
            <span>Recoveries ({recoveries.length})</span>
          </button>
        </div>
      </div>

      {/* Main SubTab Content */}
      <div>
        {activeSubTab === 'graph' && (
          <div>
            {graph ? (
              <TaskGraphView
                graph={graph}
                selectedTaskId={selectedTaskId}
                onSelectTask={(id) => setSelectedTaskId(id)}
              />
            ) : (
              <div className="p-8 text-center text-slate-400">No graph data available.</div>
            )}
          </div>
        )}

        {activeSubTab === 'tasks' && (
          <div className="bg-surface border border-surface-border rounded-lg overflow-hidden">
            <div className="p-3 border-b border-surface-border text-xs font-semibold uppercase tracking-wider text-slate-400 grid grid-cols-12 gap-2">
              <span className="col-span-4">Task Objective</span>
              <span className="col-span-2">State</span>
              <span className="col-span-1">Priority</span>
              <span className="col-span-3">Assigned Agent</span>
              <span className="col-span-2 text-right">Actions</span>
            </div>
            <div className="divide-y divide-surface-border">
              {tasks.length === 0 ? (
                <div className="p-8 text-center text-slate-400">No tasks created for this workflow.</div>
              ) : (
                tasks.map((task) => (
                  <div
                    key={task.id}
                    onClick={() => setSelectedTaskId(task.id)}
                    className="p-3 hover:bg-surface-hover cursor-pointer grid grid-cols-12 gap-2 items-center text-xs transition-colors"
                  >
                    <div className="col-span-4">
                      <div className="font-medium text-slate-200">{task.objective}</div>
                      <div className="text-[10px] font-mono text-slate-500 truncate">{task.id}</div>
                    </div>
                    <div className="col-span-2">
                      <span
                        className={`text-[10px] font-mono uppercase px-2 py-0.5 rounded border ${
                          task.state === 'Verified'
                            ? 'bg-emerald-500/10 text-emerald-400 border-emerald-500/30'
                            : task.state === 'Running'
                            ? 'bg-sky-500/10 text-sky-400 border-sky-500/30'
                            : 'bg-slate-800 text-slate-400 border-slate-700'
                        }`}
                      >
                        {task.state}
                      </span>
                    </div>
                    <div className="col-span-1 font-mono text-slate-400">P{task.priority}</div>
                    <div className="col-span-3 font-mono text-slate-400 truncate">
                      {task.assigned_agent_id ? task.assigned_agent_id.slice(0, 12) : 'Unassigned'}
                    </div>
                    <div className="col-span-2 text-right">
                      <button className="text-primary-400 hover:text-primary-300 font-medium text-[11px]">
                        Inspect →
                      </button>
                    </div>
                  </div>
                ))
              )}
            </div>
          </div>
        )}

        {activeSubTab === 'diff' && (
          <div>
            {effectiveWorkspaceId ? (
              <DiffViewer
                diff={diffData}
                loading={loadingDiff}
                onRefresh={loadDiff}
                onCommit={handleCommit}
              />
            ) : (
              <div className="bg-surface border border-surface-border rounded-lg p-8 text-center text-slate-400 font-mono text-xs">
                No workspace bound to this workflow to inspect git diff.
              </div>
            )}
          </div>
        )}

        {activeSubTab === 'messages' && (
          <div className="bg-surface border border-surface-border rounded-lg p-4 space-y-3">
            <h3 className="text-xs font-semibold uppercase tracking-wider text-slate-400">
              Inter-Agent Communication Stream
            </h3>
            {messages.length === 0 ? (
              <div className="p-8 text-center text-slate-400 font-mono text-xs">
                No inter-agent messages recorded for this workflow yet.
              </div>
            ) : (
              messages.map((m) => (
                <div key={m.id} className="p-3 bg-[#0a0d14] border border-surface-border rounded space-y-1.5">
                  <div className="flex items-center justify-between text-xs">
                    <div className="flex items-center space-x-2 font-mono">
                      <span className="text-indigo-400 font-semibold">{m.from_agent.slice(0, 8)}</span>
                      <span className="text-slate-500">→</span>
                      <span className="text-sky-400 font-semibold">{m.to_agent.slice(0, 8)}</span>
                      <span className="text-[10px] text-slate-500 px-1.5 py-0.2 bg-surface rounded">
                        {m.message_type}
                      </span>
                    </div>
                    <span className="text-[10px] font-mono text-slate-500">
                      {new Date(m.created_at).toLocaleTimeString()}
                    </span>
                  </div>
                  <p className="text-xs text-slate-300 whitespace-pre-wrap font-mono">{m.content}</p>
                </div>
              ))
            )}
          </div>
        )}

        {activeSubTab === 'verifications' && (
          <div className="bg-surface border border-surface-border rounded-lg overflow-hidden">
            <div className="divide-y divide-surface-border">
              {verifications.length === 0 ? (
                <div className="p-8 text-center text-slate-400 font-mono text-xs">
                  No independent verifications recorded yet.
                </div>
              ) : (
                verifications.map((v) => (
                  <div key={v.id} className="p-4 space-y-2">
                    <div className="flex items-center justify-between">
                      <div className="flex items-center space-x-2">
                        <span
                          className={`text-[10px] font-mono uppercase px-2 py-0.5 rounded border ${
                            v.passed
                              ? 'bg-emerald-500/10 text-emerald-400 border-emerald-500/30'
                              : 'bg-rose-500/10 text-rose-400 border-rose-500/30'
                          }`}
                        >
                          {v.passed ? 'PASSED' : 'FAILED'}
                        </span>
                        <span className="text-xs font-semibold text-slate-200">{v.verdict}</span>
                      </div>
                      <span className="text-xs font-mono text-slate-500">
                        Task: {v.task_id.slice(0, 8)} • {new Date(v.verified_at).toLocaleTimeString()}
                      </span>
                    </div>
                    {v.evidence && (
                      <div className="bg-[#0a0d14] p-2.5 rounded font-mono text-xs text-slate-300 overflow-x-auto">
                        <pre>{JSON.stringify(v.evidence, null, 2)}</pre>
                      </div>
                    )}
                  </div>
                ))
              )}
            </div>
          </div>
        )}

        {activeSubTab === 'recoveries' && (
          <div className="bg-surface border border-surface-border rounded-lg p-4 space-y-3">
            <h3 className="text-xs font-semibold uppercase tracking-wider text-slate-400">
              Recovery Diagnostic Records
            </h3>
            {recoveries.length === 0 ? (
              <div className="p-8 text-center text-slate-400 font-mono text-xs">
                No recovery attempts logged for this workflow. Execution nominal.
              </div>
            ) : (
              recoveries.map((r) => (
                <div
                  key={r.id}
                  className="p-3 bg-amber-950/20 border border-amber-500/30 rounded font-mono text-xs space-y-1"
                >
                  <div className="flex items-center justify-between text-amber-300 font-semibold">
                    <span>
                      Attempt #{r.attempt_number} • Strategy: {r.strategy}
                    </span>
                    <span className="text-[10px] uppercase">{r.status}</span>
                  </div>
                  <div className="text-slate-300">{r.diagnostics}</div>
                  <div className="text-[10px] text-slate-500">
                    Logged: {new Date(r.created_at).toLocaleString()}
                  </div>
                </div>
              ))
            )}
          </div>
        )}
      </div>

      {/* Slide-out Task Detail Drawer */}
      <TaskDetailDrawer
        taskId={selectedTaskId}
        onClose={() => setSelectedTaskId(null)}
        onSelectTask={(id) => setSelectedTaskId(id)}
        availableAgents={availableAgents}
        onTaskUpdated={loadData}
        workspaceId={effectiveWorkspaceId}
      />
    </div>
  );
};
