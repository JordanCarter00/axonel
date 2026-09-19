import React, { useState, useEffect } from 'react';
import {
  Target,
  Plus,
  Play,
  Pause,
  RotateCw,
  AlertTriangle,
  CheckCircle2,
  XCircle,
  GitCommit,
  ShieldAlert,
  RefreshCw,
  GitPullRequest,
  Eye,
  Check,
  X,
  FileText,
} from 'lucide-react';
import { api } from '../services/api';
import {
  Mission,
  MissionCheckpoint,
  MissionCycle,
  EventRecord,
  MissionState,
  MissionReviewPackage,
  Workspace,
} from '../types';

interface MissionsViewProps {
  onSelectWorkflow?: (id: string) => void;
  activeWorkspace?: Workspace | null;
}

export const MissionsView: React.FC<MissionsViewProps> = ({ onSelectWorkflow, activeWorkspace }) => {
  const [missions, setMissions] = useState<Mission[]>([]);
  const [selectedMission, setSelectedMission] = useState<Mission | null>(null);
  const [checkpoints, setCheckpoints] = useState<MissionCheckpoint[]>([]);
  const [cycles, setCycles] = useState<MissionCycle[]>([]);
  const [events, setEvents] = useState<EventRecord[]>([]);
  const [loading, setLoading] = useState(false);
  const [actionLoading, setActionLoading] = useState(false);

  // New Mission Modal State
  const [showNewModal, setShowNewModal] = useState(false);
  const [newTitle, setNewTitle] = useState('');
  const [newObjective, setNewObjective] = useState('');
  const [newMaxDuration, setNewMaxDuration] = useState(7200);
  const [newMaxExecutions, setNewMaxExecutions] = useState(25);
  const [newMaxStagnant, setNewMaxStagnant] = useState(3);
  const [newRequireTests, setNewRequireTests] = useState(true);
  const [newRequireCleanTree, setNewRequireCleanTree] = useState(true);
  const [newRequireCommit, setNewRequireCommit] = useState(true);
  const [newAutoStart, setNewAutoStart] = useState(true);
  const [newBackend, setNewBackend] = useState('gemini_cli');

  // Human Escalation Modal / Prompt State
  const [escalateReason, setEscalateReason] = useState('');
  const [showEscalatePrompt, setShowEscalatePrompt] = useState(false);

  // Review Package & Human Acceptance Modal State
  const [reviewPackage, setReviewPackage] = useState<MissionReviewPackage | null>(null);
  const [showReviewModal, setShowReviewModal] = useState(false);
  const [rejectReason, setRejectReason] = useState('');
  const [showRejectModal, setShowRejectModal] = useState(false);
  const [replanOnReject, setReplanOnReject] = useState(false);

  const fetchMissions = async () => {
    setLoading(true);
    try {
      const list = await api.listMissions();
      setMissions(list);
      if (selectedMission) {
        const updated = list.find((m) => m.id === selectedMission.id);
        if (updated) setSelectedMission(updated);
      } else if (list.length > 0) {
        setSelectedMission(list[0]);
      }
    } catch (err) {
      console.error('Failed to load missions:', err);
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    fetchMissions();
    const interval = setInterval(fetchMissions, 5000);
    return () => clearInterval(interval);
  }, []);

  const loadMissionDetails = async (m: Mission) => {
    setSelectedMission(m);
    try {
      const [ckpts, cycs, evts] = await Promise.all([
        api.listMissionCheckpoints(m.id).catch(() => []),
        api.listMissionCycles(m.id).catch(() => []),
        api.listMissionEvents(m.id).catch(() => []),
      ]);
      setCheckpoints(ckpts);
      setCycles(cycs);
      setEvents(evts);
    } catch (err) {
      console.error('Failed to load mission details:', err);
    }
  };

  useEffect(() => {
    if (selectedMission) {
      loadMissionDetails(selectedMission);
    }
  }, [selectedMission?.id]);

  const handleCreateMission = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!newTitle.trim() || !newObjective.trim()) return;

    setActionLoading(true);
    try {
      const created = await api.createMission({
        title: newTitle,
        objective: newObjective,
        workspace_id: activeWorkspace?.id || null,
        budget: {
          max_duration_secs: newMaxDuration,
          max_concurrent_agents: 4,
          max_executions: newMaxExecutions,
          max_recovery_attempts: 5,
          max_planner_iterations: 10,
          max_stagnant_cycles: newMaxStagnant,
        },
        stopping_condition: {
          required_tests_pass: newRequireTests,
          working_tree_clean: newRequireCleanTree,
          required_commit_exists: newRequireCommit,
        },
        auto_start: newAutoStart,
        metadata: {
          backend: newBackend,
        },
      });
      setShowNewModal(false);
      setNewTitle('');
      setNewObjective('');
      await fetchMissions();
      setSelectedMission(created);
    } catch (err) {
      alert(`Failed to create mission: ${err}`);
    } finally {
      setActionLoading(false);
    }
  };

  const handleStart = async (id: string) => {
    setActionLoading(true);
    try {
      const updated = await api.startMission(id);
      setSelectedMission(updated);
      await fetchMissions();
    } catch (err) {
      alert(`Start failed: ${err}`);
    } finally {
      setActionLoading(false);
    }
  };

  const handlePause = async (id: string) => {
    setActionLoading(true);
    try {
      const updated = await api.pauseMission(id);
      setSelectedMission(updated);
      await fetchMissions();
    } catch (err) {
      alert(`Pause failed: ${err}`);
    } finally {
      setActionLoading(false);
    }
  };

  const handleResume = async (id: string) => {
    setActionLoading(true);
    try {
      const updated = await api.resumeMission(id);
      setSelectedMission(updated);
      await fetchMissions();
    } catch (err) {
      alert(`Resume failed: ${err}`);
    } finally {
      setActionLoading(false);
    }
  };

  const handleStep = async (id: string) => {
    setActionLoading(true);
    try {
      const updated = await api.stepMission(id);
      setSelectedMission(updated);
      await loadMissionDetails(updated);
      await fetchMissions();
    } catch (err) {
      alert(`Step failed: ${err}`);
    } finally {
      setActionLoading(false);
    }
  };

  const handleCancel = async (id: string) => {
    if (!confirm('Are you sure you want to cancel this mission?')) return;
    setActionLoading(true);
    try {
      const updated = await api.cancelMission(id);
      setSelectedMission(updated);
      await fetchMissions();
    } catch (err) {
      alert(`Cancel failed: ${err}`);
    } finally {
      setActionLoading(false);
    }
  };

  const handleEscalate = async (id: string) => {
    if (!escalateReason.trim()) return;
    setActionLoading(true);
    try {
      const updated = await api.escalateMission(id, escalateReason);
      setSelectedMission(updated);
      setShowEscalatePrompt(false);
      setEscalateReason('');
      await fetchMissions();
    } catch (err) {
      alert(`Escalation failed: ${err}`);
    } finally {
      setActionLoading(false);
    }
  };

  const handleResolve = async (id: string, decision: 'resume' | 'replan' | 'cancel') => {
    setActionLoading(true);
    try {
      const updated = await api.resolveMission(id, decision);
      setSelectedMission(updated);
      await fetchMissions();
    } catch (err) {
      alert(`Resolution failed: ${err}`);
    } finally {
      setActionLoading(false);
    }
  };

  const handleOpenReview = async (id: string) => {
    setActionLoading(true);
    try {
      const pkg = await api.getMissionReview(id);
      setReviewPackage(pkg);
      setShowReviewModal(true);
    } catch (err) {
      alert(`Failed to load review package: ${err}`);
    } finally {
      setActionLoading(false);
    }
  };

  const handleAccept = async (id: string, integrate: boolean) => {
    setActionLoading(true);
    try {
      await api.acceptMission(id, { integrate });
      if (integrate) {
        alert(`Mission deliverable accepted and successfully integrated!`);
      } else {
        alert(`Mission deliverable accepted. Ready for integration.`);
      }
      await fetchMissions();
      if (showReviewModal) setShowReviewModal(false);
    } catch (err) {
      alert(`Acceptance failed: ${err}`);
    } finally {
      setActionLoading(false);
    }
  };

  const handleRejectConfirm = async (id: string) => {
    if (!rejectReason.trim()) return;
    setActionLoading(true);
    try {
      await api.rejectMission(id, { reason: rejectReason, continue_mission: replanOnReject });
      setShowRejectModal(false);
      setRejectReason('');
      await fetchMissions();
      if (showReviewModal) setShowReviewModal(false);
    } catch (err) {
      alert(`Rejection failed: ${err}`);
    } finally {
      setActionLoading(false);
    }
  };

  const handleIntegrate = async (id: string) => {
    setActionLoading(true);
    try {
      const res = await api.integrateMission(id);
      alert(`Integrated: ${res.integration_summary}`);
      await fetchMissions();
      if (showReviewModal) setShowReviewModal(false);
    } catch (err) {
      alert(`Integration failed: ${err}`);
    } finally {
      setActionLoading(false);
    }
  };

  const getStateBadge = (state: MissionState) => {
    switch (state) {
      case 'running':
        return 'bg-sky-500/10 text-sky-400 border-sky-500/30 animate-pulse';
      case 'planning':
        return 'bg-indigo-500/10 text-indigo-400 border-indigo-500/30';
      case 'replanning':
        return 'bg-purple-500/10 text-purple-400 border-purple-500/30 animate-pulse';
      case 'verifying':
        return 'bg-blue-500/10 text-blue-400 border-blue-500/30';
      case 'awaiting_acceptance':
        return 'bg-amber-500/20 text-amber-300 border-amber-500/40 animate-pulse';
      case 'accepted':
        return 'bg-cyan-500/20 text-cyan-300 border-cyan-500/40';
      case 'integrating':
        return 'bg-blue-500/20 text-blue-300 border-blue-500/40 animate-pulse';
      case 'integrated':
        return 'bg-emerald-500/20 text-emerald-300 border-emerald-500/40';
      case 'rejected':
        return 'bg-rose-500/20 text-rose-300 border-rose-500/40';
      case 'completed':
        return 'bg-emerald-500/10 text-emerald-400 border-emerald-500/30';
      case 'needs_human':
        return 'bg-amber-500/10 text-amber-400 border-amber-500/30 animate-pulse';
      case 'budget_exhausted':
        return 'bg-rose-500/10 text-rose-400 border-rose-500/30';
      case 'failed':
        return 'bg-rose-500/10 text-rose-400 border-rose-500/30';
      case 'waiting':
        return 'bg-slate-700/50 text-slate-300 border-slate-600';
      case 'cancelled':
        return 'bg-slate-800 text-slate-400 border-slate-700';
      default:
        return 'bg-slate-800 text-slate-400 border-slate-700';
    }
  };

  const getStateLabel = (state: MissionState, reverificationRequired?: boolean) => {
    if (reverificationRequired) return 'BLOCKED (STALE TARGET)';
    switch (state) {
      case 'awaiting_acceptance':
        return 'READY FOR REVIEW';
      case 'accepted':
        return 'ACCEPTED';
      case 'integrating':
        return 'INTEGRATING';
      case 'integrated':
        return 'INTEGRATED';
      case 'needs_human':
        return 'BLOCKED (NEEDS OPERATOR)';
      case 'rejected':
        return 'REJECTED';
      case 'running':
        return 'RUNNING';
      case 'planning':
        return 'PLANNING';
      case 'replanning':
        return 'REPLANNING';
      case 'verifying':
        return 'VERIFYING';
      case 'budget_exhausted':
        return 'BUDGET EXHAUSTED';
      case 'failed':
        return 'FAILED';
      case 'completed':
        return 'COMPLETED';
      case 'cancelled':
        return 'CANCELLED';
      default:
        return (state as string).toUpperCase();
    }
  };

  return (
    <div className="space-y-6 pb-12">
      {/* Header row */}
      <div className="flex flex-col sm:flex-row sm:items-center justify-between gap-4">
        <div>
          <h1 className="text-xl font-bold text-slate-100 flex items-center space-x-2">
            <Target className="w-5 h-5 text-indigo-400" />
            <span>Autonomous Mission Engine</span>
          </h1>
          <p className="text-xs text-slate-400">
            Durable long-horizon missions coordinating multi-cycle workflows, checkpoints, adaptive replanning, and physical stop conditions
          </p>
        </div>

        <div className="flex items-center space-x-2.5">
          <button
            onClick={fetchMissions}
            className="p-2 bg-slate-800 hover:bg-slate-700 text-slate-300 rounded-md text-xs transition-colors border border-surface-border"
            title="Refresh"
          >
            <RefreshCw className={`w-3.5 h-3.5 ${loading ? 'animate-spin' : ''}`} />
          </button>
          <button
            onClick={() => setShowNewModal(true)}
            className="flex items-center space-x-1.5 px-3.5 py-2 bg-primary-600 hover:bg-primary-500 text-white rounded-md text-xs font-semibold shadow-md shadow-indigo-600/20 transition-colors"
          >
            <Plus className="w-4 h-4" />
            <span>New Mission</span>
          </button>
        </div>
      </div>

      {/* Main layout: Missions List & Detail */}
      <div className="grid grid-cols-1 lg:grid-cols-3 gap-6">
        {/* Left column: List of missions */}
        <div className="space-y-3">
          <h2 className="text-xs font-mono uppercase tracking-wider text-slate-400">
            Active & Past Missions ({missions.length})
          </h2>

          {missions.length === 0 && (
            <div className="p-8 border border-dashed border-surface-border rounded-lg text-center">
              <Target className="w-8 h-8 text-slate-600 mx-auto mb-2" />
              <p className="text-xs text-slate-400 font-medium">No missions created yet</p>
              <button
                onClick={() => setShowNewModal(true)}
                className="mt-3 text-xs text-indigo-400 hover:text-indigo-300 font-semibold"
              >
                + Launch your first mission
              </button>
            </div>
          )}

          {missions.map((m) => {
            const isSelected = selectedMission?.id === m.id;
            return (
              <div
                key={m.id}
                onClick={() => loadMissionDetails(m)}
                className={`p-4 rounded-lg border cursor-pointer transition-all ${
                  isSelected
                    ? 'bg-slate-800/80 border-indigo-500/50 shadow-md'
                    : 'bg-[#121829] border-surface-border hover:border-slate-600'
                }`}
              >
                <div className="flex items-start justify-between gap-2 mb-1.5">
                  <h3 className="text-sm font-semibold text-slate-200 line-clamp-1">
                    {m.title}
                  </h3>
                  <span
                    className={`px-2 py-0.5 rounded text-[10px] font-mono uppercase font-bold border ${getStateBadge(
                      m.state
                    )}`}
                  >
                    {getStateLabel(m.state)}
                  </span>
                </div>

                <p className="text-xs text-slate-400 line-clamp-2 mb-3">
                  {m.objective}
                </p>

                <div className="flex items-center justify-between text-[11px] font-mono text-slate-400 pt-2 border-t border-slate-700/50">
                  <span className="flex items-center space-x-1">
                    <RotateCw className="w-3 h-3 text-indigo-400" />
                    <span>Cycle {m.cycle_index + 1}</span>
                  </span>

                  {m.budget_consumed.stagnant_cycles > 0 && (
                    <span className="text-amber-400 flex items-center space-x-1">
                      <AlertTriangle className="w-3 h-3" />
                      <span>Stagnant: {m.budget_consumed.stagnant_cycles}</span>
                    </span>
                  )}

                  {m.latest_verified_commit && (
                    <span className="text-emerald-400 flex items-center space-x-1">
                      <GitCommit className="w-3 h-3" />
                      <span>{m.latest_verified_commit.slice(0, 7)}</span>
                    </span>
                  )}
                </div>
              </div>
            );
          })}
        </div>

        {/* Right column: Selected Mission Details */}
        <div className="lg:col-span-2 space-y-6">
          {selectedMission ? (
            <div className="bg-[#121829] border border-surface-border rounded-lg p-6 space-y-6">
              {/* Top Details & Controls */}
              <div className="flex flex-col sm:flex-row sm:items-center justify-between gap-4 pb-4 border-b border-surface-border">
                <div>
                  <div className="flex items-center space-x-2 mb-1">
                    <h2 className="text-lg font-bold text-slate-100">
                      {selectedMission.title}
                    </h2>
                    <span
                      className={`px-2 py-0.5 rounded text-[10px] font-mono uppercase font-bold border ${getStateBadge(
                        selectedMission.state
                      )}`}
                    >
                      {getStateLabel(selectedMission.state)}
                    </span>
                  </div>
                  <div className="flex items-center space-x-2 text-xs text-slate-400 font-mono">
                    <span>ID: {selectedMission.id}</span>
                    {selectedMission.active_workflow_id && onSelectWorkflow && (
                      <>
                        <span>•</span>
                        <button
                          onClick={() => onSelectWorkflow(selectedMission.active_workflow_id!)}
                          className="text-indigo-400 hover:text-indigo-300 underline"
                        >
                          Workflow {selectedMission.active_workflow_id}
                        </button>
                      </>
                    )}
                  </div>
                </div>

                {/* Control Action Buttons */}
                <div className="flex items-center space-x-2">
                  {selectedMission.state === 'created' && (
                    <button
                      disabled={actionLoading}
                      onClick={() => handleStart(selectedMission.id)}
                      className="flex items-center space-x-1 px-3 py-1.5 bg-emerald-600 hover:bg-emerald-500 text-white rounded text-xs font-semibold shadow transition-colors"
                    >
                      <Play className="w-3.5 h-3.5" />
                      <span>Start</span>
                    </button>
                  )}

                  {(selectedMission.state === 'running' || selectedMission.state === 'planning') && (
                    <>
                      <button
                        disabled={actionLoading}
                        onClick={() => handleStep(selectedMission.id)}
                        className="flex items-center space-x-1 px-3 py-1.5 bg-indigo-600 hover:bg-indigo-500 text-white rounded text-xs font-semibold shadow transition-colors"
                        title="Execute next autonomous cycle step"
                      >
                        <RotateCw className={`w-3.5 h-3.5 ${actionLoading ? 'animate-spin' : ''}`} />
                        <span>Step Cycle</span>
                      </button>

                      <button
                        disabled={actionLoading}
                        onClick={() => handlePause(selectedMission.id)}
                        className="flex items-center space-x-1 px-3 py-1.5 bg-amber-600/80 hover:bg-amber-600 text-white rounded text-xs font-semibold transition-colors"
                      >
                        <Pause className="w-3.5 h-3.5" />
                        <span>Pause</span>
                      </button>
                    </>
                  )}

                  {selectedMission.state === 'waiting' && (
                    <button
                      disabled={actionLoading}
                      onClick={() => handleResume(selectedMission.id)}
                      className="flex items-center space-x-1 px-3 py-1.5 bg-emerald-600 hover:bg-emerald-500 text-white rounded text-xs font-semibold transition-colors"
                    >
                      <Play className="w-3.5 h-3.5" />
                      <span>Resume</span>
                    </button>
                  )}

                  {selectedMission.state === 'awaiting_acceptance' && (
                    <>
                      <button
                        disabled={actionLoading}
                        onClick={() => handleOpenReview(selectedMission.id)}
                        className="flex items-center space-x-1 px-3 py-1.5 bg-slate-800 hover:bg-slate-700 text-slate-200 border border-slate-600 rounded text-xs font-semibold transition-colors"
                      >
                        <Eye className="w-3.5 h-3.5" />
                        <span>Review</span>
                      </button>
                      <button
                        disabled={actionLoading}
                        onClick={() => handleAccept(selectedMission.id, true)}
                        className="flex items-center space-x-1 px-3 py-1.5 bg-emerald-600 hover:bg-emerald-500 text-white rounded text-xs font-semibold shadow transition-colors"
                      >
                        <Check className="w-3.5 h-3.5" />
                        <span>Accept & Integrate</span>
                      </button>
                      <button
                        disabled={actionLoading}
                        onClick={() => {
                          setRejectReason('');
                          setReplanOnReject(false);
                          setShowRejectModal(true);
                        }}
                        className="flex items-center space-x-1 px-3 py-1.5 bg-rose-600/20 hover:bg-rose-600/30 text-rose-400 border border-rose-500/30 rounded text-xs font-semibold transition-colors"
                      >
                        <X className="w-3.5 h-3.5" />
                        <span>Reject</span>
                      </button>
                    </>
                  )}

                  {selectedMission.state === 'accepted' && (
                    <>
                      <button
                        disabled={actionLoading}
                        onClick={() => handleIntegrate(selectedMission.id)}
                        className="flex items-center space-x-1 px-3 py-1.5 bg-emerald-600 hover:bg-emerald-500 text-white rounded text-xs font-semibold shadow transition-colors"
                      >
                        <GitPullRequest className="w-3.5 h-3.5" />
                        <span>Integrate</span>
                      </button>
                      <button
                        disabled={actionLoading}
                        onClick={() => {
                          setRejectReason('');
                          setReplanOnReject(false);
                          setShowRejectModal(true);
                        }}
                        className="flex items-center space-x-1 px-3 py-1.5 bg-rose-600/20 hover:bg-rose-600/30 text-rose-400 border border-rose-500/30 rounded text-xs font-semibold transition-colors"
                      >
                        <X className="w-3.5 h-3.5" />
                        <span>Reject</span>
                      </button>
                    </>
                  )}

                  {!['completed', 'integrated', 'rejected', 'failed', 'cancelled', 'budget_exhausted'].includes(
                    selectedMission.state
                  ) && (
                    <>
                      <button
                        onClick={() => setShowEscalatePrompt(true)}
                        className="flex items-center space-x-1 px-3 py-1.5 bg-slate-700 hover:bg-slate-600 text-amber-300 rounded text-xs font-semibold transition-colors border border-amber-500/30"
                      >
                        <ShieldAlert className="w-3.5 h-3.5" />
                        <span>Escalate</span>
                      </button>

                      <button
                        disabled={actionLoading}
                        onClick={() => handleCancel(selectedMission.id)}
                        className="flex items-center space-x-1 px-3 py-1.5 bg-rose-600/20 hover:bg-rose-600/30 text-rose-400 border border-rose-500/30 rounded text-xs font-semibold transition-colors"
                      >
                        <XCircle className="w-3.5 h-3.5" />
                        <span>Cancel</span>
                      </button>
                    </>
                  )}
                </div>
              </div>

              {/* Review Ready / Awaiting Human Acceptance Banner */}
              {selectedMission.state === 'awaiting_acceptance' && (
                <div className="p-4 bg-amber-500/10 border border-amber-500/40 rounded-lg space-y-3">
                  <div className="flex items-start justify-between">
                    <div className="flex items-start space-x-2.5">
                      <CheckCircle2 className="w-5 h-5 text-amber-400 shrink-0 mt-0.5" />
                      <div>
                        <h4 className="text-sm font-semibold text-amber-300 flex items-center space-x-2">
                          <span>Physical Verification Passed — Awaiting Human Acceptance</span>
                          <span className="px-2 py-0.5 rounded text-[10px] font-mono bg-amber-500/20 text-amber-300 border border-amber-500/40">
                            REVIEW READY
                          </span>
                        </h4>
                        <p className="text-xs text-amber-200/90 mt-1">
                          Independent on-disk verifier confirmed all stopping conditions passed at commit{' '}
                          <code className="px-1 py-0.5 bg-slate-900 rounded font-mono text-amber-300">
                            {selectedMission.latest_verified_commit || 'HEAD'}
                          </code>
                          . In accordance with Axonel V1 release contract, autonomous execution has halted. Changes will not merge without explicit acceptance.
                        </p>
                      </div>
                    </div>
                  </div>

                  <div className="flex items-center space-x-2 pt-2 border-t border-amber-500/20">
                    <button
                      disabled={actionLoading}
                      onClick={() => handleOpenReview(selectedMission.id)}
                      className="flex items-center space-x-1 px-3 py-1.5 bg-slate-800 hover:bg-slate-700 text-slate-200 rounded text-xs font-semibold border border-slate-600 transition-colors"
                    >
                      <Eye className="w-3.5 h-3.5" />
                      <span>Inspect Review Package</span>
                    </button>
                    <button
                      disabled={actionLoading}
                      onClick={() => handleAccept(selectedMission.id, true)}
                      className="flex items-center space-x-1 px-3 py-1.5 bg-emerald-600 hover:bg-emerald-500 text-white rounded text-xs font-semibold shadow transition-colors"
                    >
                      <Check className="w-3.5 h-3.5" />
                      <span>Accept & Integrate</span>
                    </button>
                    <button
                      disabled={actionLoading}
                      onClick={() => handleAccept(selectedMission.id, false)}
                      className="flex items-center space-x-1 px-3 py-1.5 bg-cyan-600/80 hover:bg-cyan-600 text-white rounded text-xs font-semibold transition-colors"
                    >
                      <span>Accept Only</span>
                    </button>
                    <button
                      disabled={actionLoading}
                      onClick={() => {
                        setRejectReason('');
                        setReplanOnReject(false);
                        setShowRejectModal(true);
                      }}
                      className="flex items-center space-x-1 px-3 py-1.5 bg-rose-600/20 hover:bg-rose-600/30 text-rose-400 border border-rose-500/30 rounded text-xs font-semibold transition-colors"
                    >
                      <X className="w-3.5 h-3.5" />
                      <span>Reject</span>
                    </button>
                  </div>
                </div>
              )}

              {/* Accepted / Ready to Integrate Banner */}
              {selectedMission.state === 'accepted' && (
                <div className="p-4 bg-cyan-500/10 border border-cyan-500/40 rounded-lg space-y-3">
                  <div className="flex items-start justify-between">
                    <div className="flex items-start space-x-2.5">
                      <Check className="w-5 h-5 text-cyan-400 shrink-0 mt-0.5" />
                      <div>
                        <h4 className="text-sm font-semibold text-cyan-300">
                          Deliverable Accepted — Ready for Safe Integration
                        </h4>
                        <p className="text-xs text-cyan-200/90 mt-1">
                          Human acceptance is recorded. Verified commit{' '}
                          <code className="px-1 py-0.5 bg-slate-900 rounded font-mono text-cyan-300">
                            {selectedMission.latest_verified_commit || 'HEAD'}
                          </code>{' '}
                          is eligible for branch integration.
                        </p>
                      </div>
                    </div>
                    <button
                      disabled={actionLoading}
                      onClick={() => handleIntegrate(selectedMission.id)}
                      className="flex items-center space-x-1.5 px-4 py-2 bg-emerald-600 hover:bg-emerald-500 text-white rounded text-xs font-bold shadow-lg transition-colors"
                    >
                      <GitPullRequest className="w-4 h-4" />
                      <span>Integrate into Target Branch</span>
                    </button>
                  </div>
                </div>
              )}

              {/* Integrated Banner */}
              {selectedMission.state === 'integrated' && (
                <div className="p-4 bg-emerald-500/10 border border-emerald-500/40 rounded-lg flex items-center justify-between">
                  <div className="flex items-center space-x-2.5">
                    <CheckCircle2 className="w-5 h-5 text-emerald-400 shrink-0" />
                    <div>
                      <h4 className="text-sm font-semibold text-emerald-300">
                        Deliverable Integrated Successfully
                      </h4>
                      <p className="text-xs text-emerald-200/90">
                        Verified commit {selectedMission.latest_verified_commit} has been safely integrated into repository branch.
                      </p>
                    </div>
                  </div>
                  <span className="px-2 py-1 bg-emerald-500/20 text-emerald-300 border border-emerald-500/40 rounded text-xs font-mono font-bold">
                    MERGED & ARCHIVED
                  </span>
                </div>
              )}

              {/* Rejected Banner */}
              {selectedMission.state === 'rejected' && (
                <div className="p-4 bg-rose-500/10 border border-rose-500/40 rounded-lg flex items-center justify-between">
                  <div className="flex items-center space-x-2.5">
                    <XCircle className="w-5 h-5 text-rose-400 shrink-0" />
                    <div>
                      <h4 className="text-sm font-semibold text-rose-300">
                        Deliverable Rejected (Non-destructive Audit)
                      </h4>
                      <p className="text-xs text-rose-200/90">
                        Reason: {String((selectedMission as any).metadata?.rejection_reason || 'Rejected by operator')}
                      </p>
                    </div>
                  </div>
                </div>
              )}

              {/* Human Escalation Resolution Banner */}
              {selectedMission.state === 'needs_human' && (
                <div className="p-4 bg-amber-500/10 border border-amber-500/40 rounded-lg space-y-3 animate-pulse">
                  <div className="flex items-start space-x-2.5">
                    <ShieldAlert className="w-5 h-5 text-amber-400 shrink-0 mt-0.5" />
                    <div>
                      <h4 className="text-sm font-semibold text-amber-300">
                        Human Intervention Required
                      </h4>
                      <p className="text-xs text-amber-200/90 mt-1">
                        {selectedMission.escalation_reason ||
                          'Mission halted pending operator review or guidance.'}
                      </p>
                    </div>
                  </div>

                  <div className="flex items-center space-x-2 pt-2 border-t border-amber-500/20">
                    <span className="text-xs font-mono text-slate-300">Operator Decision:</span>
                    <button
                      disabled={actionLoading}
                      onClick={() => handleResolve(selectedMission.id, 'resume')}
                      className="px-3 py-1 bg-emerald-600 hover:bg-emerald-500 text-white rounded text-xs font-semibold"
                    >
                      Resume
                    </button>
                    <button
                      disabled={actionLoading}
                      onClick={() => handleResolve(selectedMission.id, 'replan')}
                      className="px-3 py-1 bg-indigo-600 hover:bg-indigo-500 text-white rounded text-xs font-semibold"
                    >
                      Replan
                    </button>
                    <button
                      disabled={actionLoading}
                      onClick={() => handleResolve(selectedMission.id, 'cancel')}
                      className="px-3 py-1 bg-rose-600 hover:bg-rose-500 text-white rounded text-xs font-semibold"
                    >
                      Cancel Mission
                    </button>
                  </div>
                </div>
              )}

              {/* Manual Escalate Prompt Dialog */}
              {showEscalatePrompt && (
                <div className="p-4 bg-slate-800 border border-slate-700 rounded-lg space-y-3">
                  <h4 className="text-xs font-semibold text-slate-200">
                    Escalate Mission to Human Operator
                  </h4>
                  <input
                    type="text"
                    placeholder="Reason for escalation (e.g. clarification needed, blocked on review)..."
                    value={escalateReason}
                    onChange={(e) => setEscalateReason(e.target.value)}
                    className="w-full px-3 py-2 bg-slate-900 border border-slate-700 rounded text-xs text-slate-200 focus:outline-none focus:border-indigo-500"
                  />
                  <div className="flex items-center justify-end space-x-2">
                    <button
                      onClick={() => setShowEscalatePrompt(false)}
                      className="px-3 py-1 text-xs text-slate-400 hover:text-slate-200"
                    >
                      Cancel
                    </button>
                    <button
                      onClick={() => handleEscalate(selectedMission.id)}
                      className="px-3 py-1 bg-amber-600 hover:bg-amber-500 text-white rounded text-xs font-semibold"
                    >
                      Confirm Escalation
                    </button>
                  </div>
                </div>
              )}

              {/* Objective Description */}
              <div className="space-y-1.5">
                <span className="text-xs font-mono uppercase tracking-wider text-slate-400">
                  Mission Objective
                </span>
                <p className="text-sm text-slate-200 bg-slate-900/50 p-3 rounded border border-surface-border">
                  {selectedMission.objective}
                </p>
              </div>

              {/* Verified Stopping Conditions & Outcome */}
              <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
                {/* Stopping Conditions */}
                <div className="p-4 bg-slate-900/40 border border-surface-border rounded-lg space-y-2">
                  <span className="text-xs font-mono uppercase tracking-wider text-slate-400">
                    Stopping Conditions
                  </span>
                  <div className="space-y-1.5 text-xs">
                    <div className="flex items-center space-x-2">
                      {selectedMission.stopping_condition.required_tests_pass ? (
                        <CheckCircle2 className="w-3.5 h-3.5 text-emerald-400" />
                      ) : (
                        <span className="w-3.5 h-3.5 rounded-full border border-slate-600" />
                      )}
                      <span className="text-slate-300">Automated Tests Pass (cargo test = 0)</span>
                    </div>

                    <div className="flex items-center space-x-2">
                      {selectedMission.stopping_condition.working_tree_clean ? (
                        <CheckCircle2 className="w-3.5 h-3.5 text-emerald-400" />
                      ) : (
                        <span className="w-3.5 h-3.5 rounded-full border border-slate-600" />
                      )}
                      <span className="text-slate-300">Clean Working Tree (git porcelain)</span>
                    </div>

                    <div className="flex items-center space-x-2">
                      {selectedMission.stopping_condition.required_commit_exists ? (
                        <CheckCircle2 className="w-3.5 h-3.5 text-emerald-400" />
                      ) : (
                        <span className="w-3.5 h-3.5 rounded-full border border-slate-600" />
                      )}
                      <span className="text-slate-300">Valid Git Commit SHA Exists</span>
                    </div>
                  </div>
                </div>

                {/* Verified Git Artifact */}
                <div className="p-4 bg-slate-900/40 border border-surface-border rounded-lg space-y-2">
                  <span className="text-xs font-mono uppercase tracking-wider text-slate-400">
                    Verified Git Provenance
                  </span>
                  {selectedMission.latest_verified_commit ? (
                    <div className="space-y-2">
                      <div className="flex items-center space-x-2 text-emerald-400">
                        <GitCommit className="w-4 h-4" />
                        <span className="font-mono text-xs font-bold">
                          {selectedMission.latest_verified_commit}
                        </span>
                      </div>
                      <p className="text-[11px] text-slate-400">
                        Commit verified against repository HEAD with independent quality gates.
                      </p>
                    </div>
                  ) : (
                    <p className="text-xs text-slate-500 font-mono">
                      No verified commit produced yet in current cycle.
                    </p>
                  )}
                </div>
              </div>

              {/* Budget Consumption Progress */}
              <div className="p-4 bg-slate-900/40 border border-surface-border rounded-lg space-y-3">
                <span className="text-xs font-mono uppercase tracking-wider text-slate-400">
                  Resource Budget Consumption
                </span>

                <div className="grid grid-cols-2 sm:grid-cols-4 gap-4">
                  {/* Executions */}
                  <div>
                    <div className="flex justify-between text-[11px] font-mono text-slate-400 mb-1">
                      <span>Executions</span>
                      <span>
                        {selectedMission.budget_consumed.total_executions} /{' '}
                        {selectedMission.budget.max_executions}
                      </span>
                    </div>
                    <div className="w-full bg-slate-800 h-1.5 rounded-full overflow-hidden">
                      <div
                        className="bg-indigo-500 h-full rounded-full"
                        style={{
                          width: `${Math.min(
                            100,
                            (selectedMission.budget_consumed.total_executions /
                              selectedMission.budget.max_executions) *
                              100
                          )}%`,
                        }}
                      />
                    </div>
                  </div>

                  {/* Planner Iterations */}
                  <div>
                    <div className="flex justify-between text-[11px] font-mono text-slate-400 mb-1">
                      <span>Planner Iterations</span>
                      <span>
                        {selectedMission.budget_consumed.planner_iterations} /{' '}
                        {selectedMission.budget.max_planner_iterations}
                      </span>
                    </div>
                    <div className="w-full bg-slate-800 h-1.5 rounded-full overflow-hidden">
                      <div
                        className="bg-purple-500 h-full rounded-full"
                        style={{
                          width: `${Math.min(
                            100,
                            (selectedMission.budget_consumed.planner_iterations /
                              selectedMission.budget.max_planner_iterations) *
                              100
                          )}%`,
                        }}
                      />
                    </div>
                  </div>

                  {/* Stagnant Cycles */}
                  <div>
                    <div className="flex justify-between text-[11px] font-mono text-slate-400 mb-1">
                      <span>Stagnant Cycles</span>
                      <span className={selectedMission.budget_consumed.stagnant_cycles > 0 ? 'text-amber-400 font-bold' : ''}>
                        {selectedMission.budget_consumed.stagnant_cycles} /{' '}
                        {selectedMission.budget.max_stagnant_cycles}
                      </span>
                    </div>
                    <div className="w-full bg-slate-800 h-1.5 rounded-full overflow-hidden">
                      <div
                        className="bg-amber-500 h-full rounded-full"
                        style={{
                          width: `${Math.min(
                            100,
                            (selectedMission.budget_consumed.stagnant_cycles /
                              selectedMission.budget.max_stagnant_cycles) *
                              100
                          )}%`,
                        }}
                      />
                    </div>
                  </div>

                  {/* Duration */}
                  <div>
                    <div className="flex justify-between text-[11px] font-mono text-slate-400 mb-1">
                      <span>Duration</span>
                      <span>
                        {selectedMission.budget_consumed.duration_secs}s /{' '}
                        {selectedMission.budget.max_duration_secs}s
                      </span>
                    </div>
                    <div className="w-full bg-slate-800 h-1.5 rounded-full overflow-hidden">
                      <div
                        className="bg-blue-500 h-full rounded-full"
                        style={{
                          width: `${Math.min(
                            100,
                            (selectedMission.budget_consumed.duration_secs /
                              selectedMission.budget.max_duration_secs) *
                              100
                          )}%`,
                        }}
                      />
                    </div>
                  </div>
                </div>
              </div>

              {/* Checkpoints & Execution Cycles */}
              <div className="space-y-4">
                <h3 className="text-xs font-mono uppercase tracking-wider text-slate-400">
                  Durable Checkpoints ({checkpoints.length})
                </h3>

                {checkpoints.length === 0 ? (
                  <p className="text-xs text-slate-500 font-mono italic">
                    No checkpoints recorded yet for this mission.
                  </p>
                ) : (
                  <div className="space-y-2">
                    {checkpoints.map((ckpt) => (
                      <div
                        key={ckpt.id}
                        className="p-3 bg-slate-900/60 border border-surface-border rounded flex items-center justify-between text-xs"
                      >
                        <div className="flex items-center space-x-3">
                          <span className="font-mono text-indigo-400 font-bold">
                            Cycle {ckpt.cycle_index}
                          </span>
                          <span className="text-slate-400 font-mono text-[11px]">
                            {ckpt.id}
                          </span>
                          <span className="text-emerald-400 text-[11px]">
                            ✓ {ckpt.completed_tasks.length} completed
                          </span>
                          {ckpt.unresolved_tasks.length > 0 && (
                            <span className="text-slate-400 text-[11px]">
                              ⏳ {ckpt.unresolved_tasks.length} unresolved
                            </span>
                          )}
                        </div>

                        {ckpt.latest_verified_commit && (
                          <div className="flex items-center space-x-1 text-slate-300 font-mono text-[11px]">
                            <GitCommit className="w-3 h-3 text-emerald-400" />
                            <span>{ckpt.latest_verified_commit.slice(0, 7)}</span>
                          </div>
                        )}
                      </div>
                    ))}
                  </div>
                )}
              </div>

              {/* Cycles History */}
              {cycles.length > 0 && (
                <div className="space-y-2">
                  <h3 className="text-xs font-mono uppercase tracking-wider text-slate-400">
                    Execution Cycles History ({cycles.length})
                  </h3>
                  <div className="space-y-1.5">
                    {cycles.map((c) => (
                      <div
                        key={c.id}
                        className="p-2.5 bg-slate-900/40 border border-surface-border rounded flex items-center justify-between text-xs"
                      >
                        <div className="flex items-center space-x-2">
                          <span className="font-mono text-indigo-400">Cycle {c.cycle_index}</span>
                          <span className="text-slate-300">{c.summary}</span>
                        </div>
                        <span className="text-[11px] font-mono text-slate-400 uppercase">
                          {c.outcome}
                        </span>
                      </div>
                    ))}
                  </div>
                </div>
              )}

              {/* Events Stream */}
              {events.length > 0 && (
                <div className="space-y-2">
                  <h3 className="text-xs font-mono uppercase tracking-wider text-slate-400">
                    Recent Mission Events ({events.length})
                  </h3>
                  <div className="max-h-48 overflow-y-auto space-y-1.5 bg-slate-900/30 p-2 rounded border border-surface-border font-mono text-[11px]">
                    {events.map((ev) => (
                      <div key={ev.sequence} className="flex items-center justify-between text-slate-400">
                        <span className="text-indigo-400 font-bold">{ev.event_type}</span>
                        <span className="text-slate-500 text-[10px]">
                          {ev.timestamp ? new Date(ev.timestamp).toLocaleTimeString() : ''}
                        </span>
                      </div>
                    ))}
                  </div>
                </div>
              )}
            </div>
          ) : (
            <div className="p-12 border border-surface-border rounded-lg text-center text-slate-500">
              Select a mission from the list to view telemetry and lifecycle controls
            </div>
          )}
        </div>
      </div>

      {/* New Mission Modal */}
      {showNewModal && (
        <div className="fixed inset-0 z-50 bg-black/70 backdrop-blur-sm flex items-center justify-center p-4">
          <div className="bg-[#121829] border border-surface-border rounded-xl max-w-lg w-full p-6 space-y-4 shadow-2xl">
            <div className="flex items-center justify-between pb-3 border-b border-surface-border">
              <h3 className="text-base font-bold text-slate-100 flex items-center space-x-2">
                <Target className="w-4 h-4 text-indigo-400" />
                <span>Launch Autonomous Mission</span>
              </h3>
              <button
                onClick={() => setShowNewModal(false)}
                className="text-slate-400 hover:text-slate-200 text-sm"
              >
                ✕
              </button>
            </div>

            <form onSubmit={handleCreateMission} className="space-y-4 text-xs">
              {/* Workspace indicator */}
              <div className="bg-slate-950 p-2.5 rounded border border-slate-800">
                <span className="block text-slate-400 font-mono text-[11px] mb-0.5">Target Workspace</span>
                <div className="text-slate-200 font-mono text-xs truncate">
                  {activeWorkspace ? `${activeWorkspace.name} (${activeWorkspace.canonical_path})` : 'No workspace selected (global)'}
                </div>
              </div>

              {/* Agent Backend Selector */}
              <div>
                <label className="block text-slate-300 font-semibold mb-1">
                  Agent Backend
                </label>
                <select
                  value={newBackend}
                  onChange={(e) => setNewBackend(e.target.value)}
                  className="w-full px-3 py-2 bg-slate-900 border border-slate-700 rounded text-slate-200 focus:outline-none focus:border-indigo-500 font-mono text-xs"
                >
                  <option value="gemini_cli">Google Gemini CLI (Proven)</option>
                  <option value="fake_agent">Test Agent (Deterministic Mock)</option>
                  <option value="echo">Echo Agent (Diagnostics)</option>
                </select>
              </div>

              <div>
                <label className="block text-slate-300 font-semibold mb-1">
                  Mission Title
                </label>
                <input
                  type="text"
                  required
                  placeholder="e.g. Long-Horizon Authentication Refactor"
                  value={newTitle}
                  onChange={(e) => setNewTitle(e.target.value)}
                  className="w-full px-3 py-2 bg-slate-900 border border-slate-700 rounded text-slate-200 focus:outline-none focus:border-indigo-500"
                />
              </div>

              <div>
                <label className="block text-slate-300 font-semibold mb-1">
                  High-Level Objective
                </label>
                <textarea
                  required
                  rows={3}
                  placeholder="Describe the software engineering objective to sustain autonomously across multiple cycles until verified..."
                  value={newObjective}
                  onChange={(e) => setNewObjective(e.target.value)}
                  className="w-full px-3 py-2 bg-slate-900 border border-slate-700 rounded text-slate-200 focus:outline-none focus:border-indigo-500"
                />
              </div>

              <div className="grid grid-cols-3 gap-3">
                <div>
                  <label className="block text-slate-400 mb-1 font-mono text-[11px]">
                    Max Duration (s)
                  </label>
                  <input
                    type="number"
                    value={newMaxDuration}
                    onChange={(e) => setNewMaxDuration(Number(e.target.value))}
                    className="w-full px-2.5 py-1.5 bg-slate-900 border border-slate-700 rounded text-slate-200 font-mono text-xs"
                  />
                </div>
                <div>
                  <label className="block text-slate-400 mb-1 font-mono text-[11px]">
                    Max Executions
                  </label>
                  <input
                    type="number"
                    value={newMaxExecutions}
                    onChange={(e) => setNewMaxExecutions(Number(e.target.value))}
                    className="w-full px-2.5 py-1.5 bg-slate-900 border border-slate-700 rounded text-slate-200 font-mono text-xs"
                  />
                </div>
                <div>
                  <label className="block text-slate-400 mb-1 font-mono text-[11px]">
                    Stagnation Bound
                  </label>
                  <input
                    type="number"
                    value={newMaxStagnant}
                    onChange={(e) => setNewMaxStagnant(Number(e.target.value))}
                    className="w-full px-2.5 py-1.5 bg-slate-900 border border-slate-700 rounded text-slate-200 font-mono text-xs"
                  />
                </div>
              </div>

              <div className="space-y-2 pt-2 border-t border-slate-800">
                <span className="block text-slate-400 font-mono text-[11px]">
                  Physical Stopping Conditions
                </span>
                <label className="flex items-center space-x-2 text-slate-300">
                  <input
                    type="checkbox"
                    checked={newRequireTests}
                    onChange={(e) => setNewRequireTests(e.target.checked)}
                    className="rounded bg-slate-900 border-slate-700 text-indigo-500"
                  />
                  <span>Automated test suite passes (`cargo test = 0`)</span>
                </label>
                <label className="flex items-center space-x-2 text-slate-300">
                  <input
                    type="checkbox"
                    checked={newRequireCleanTree}
                    onChange={(e) => setNewRequireCleanTree(e.target.checked)}
                    className="rounded bg-slate-900 border-slate-700 text-indigo-500"
                  />
                  <span>Working tree is completely clean</span>
                </label>
                <label className="flex items-center space-x-2 text-slate-300">
                  <input
                    type="checkbox"
                    checked={newRequireCommit}
                    onChange={(e) => setNewRequireCommit(e.target.checked)}
                    className="rounded bg-slate-900 border-slate-700 text-indigo-500"
                  />
                  <span>Valid Git commit SHA exists at repository HEAD</span>
                </label>
              </div>

              <div className="pt-2 border-t border-slate-800">
                <label className="flex items-center space-x-2 text-slate-300">
                  <input
                    type="checkbox"
                    checked={newAutoStart}
                    onChange={(e) => setNewAutoStart(e.target.checked)}
                    className="rounded bg-slate-900 border-slate-700 text-indigo-500"
                  />
                  <span>Auto-start mission immediately upon creation</span>
                </label>
              </div>

              <div className="flex items-center justify-end space-x-3 pt-4 border-t border-slate-800">
                <button
                  type="button"
                  onClick={() => setShowNewModal(false)}
                  className="px-4 py-2 text-xs text-slate-400 hover:text-slate-200"
                >
                  Cancel
                </button>
                <button
                  type="submit"
                  disabled={actionLoading}
                  className="px-4 py-2 bg-primary-600 hover:bg-primary-500 text-white rounded text-xs font-semibold shadow transition-colors"
                >
                  {actionLoading ? 'Creating...' : 'Launch Mission'}
                </button>
              </div>
            </form>
          </div>
        </div>
      )}

      {/* Review Package Modal */}
      {showReviewModal && reviewPackage && (
        <div className="fixed inset-0 bg-black/80 z-50 flex items-center justify-center p-4 backdrop-blur-sm animate-fade-in">
          <div className="bg-slate-900 border border-slate-700 rounded-xl max-w-4xl w-full max-h-[90vh] flex flex-col shadow-2xl overflow-hidden">
            {/* Modal Header */}
            <div className="p-4 border-b border-slate-800 flex items-center justify-between bg-slate-950/50">
              <div className="flex items-center space-x-3">
                <div className="p-2 bg-indigo-500/10 border border-indigo-500/30 rounded-lg text-indigo-400">
                  <FileText className="w-5 h-5" />
                </div>
                <div>
                  <div className="flex items-center space-x-2">
                    <h3 className="font-semibold text-slate-100 text-sm">Mission Review Package</h3>
                    <span className={`px-2 py-0.5 rounded-full text-[10px] font-mono font-medium border ${getStateBadge(reviewPackage.status)}`}>
                      {getStateLabel(reviewPackage.status, reviewPackage.reverification_required)}
                    </span>
                  </div>
                  <p className="text-xs text-slate-400 font-mono mt-0.5">
                    ID: {reviewPackage.mission_id} · Target: {reviewPackage.target_branch || 'main'}
                  </p>
                </div>
              </div>
              <button
                onClick={() => setShowReviewModal(false)}
                className="text-slate-400 hover:text-slate-200 p-1.5 rounded-lg hover:bg-slate-800 transition-colors"
              >
                <X className="w-5 h-5" />
              </button>
            </div>

            {/* Modal Body */}
            <div className="p-6 space-y-5 overflow-y-auto flex-1 font-sans text-xs">
              {/* Warnings Banner */}
              {reviewPackage.warnings && reviewPackage.warnings.length > 0 && (
                <div className="p-3 bg-amber-500/10 border border-amber-500/30 rounded-lg space-y-1">
                  <div className="flex items-center space-x-2 text-amber-400 font-medium">
                    <AlertTriangle className="w-4 h-4" />
                    <span>Review Warnings</span>
                  </div>
                  <ul className="list-disc list-inside text-amber-300/90 text-[11px] space-y-0.5 pl-1">
                    {reviewPackage.warnings.map((w, idx) => (
                      <li key={idx}>{w}</li>
                    ))}
                  </ul>
                </div>
              )}

              {/* Target Branch Freshness Alert */}
              {reviewPackage.reverification_required && (
                <div className="p-3 bg-amber-500/10 border border-amber-500/30 rounded-lg flex items-center space-x-2 text-amber-300 text-xs">
                  <AlertTriangle className="w-4 h-4 shrink-0 text-amber-400" />
                  <span>
                    <strong>Target Branch Moved:</strong> Target HEAD has changed since verification (verified: {reviewPackage.verified_target_head ? reviewPackage.verified_target_head.slice(0, 8) : 'unknown'}, current: {reviewPackage.current_target_head ? reviewPackage.current_target_head.slice(0, 8) : 'unknown'}). Re-verification is recommended before integration.
                  </span>
                </div>
              )}

              {/* Objective */}
              <div className="bg-slate-950 p-3.5 rounded-lg border border-slate-800">
                <span className="text-[11px] font-mono text-slate-400 block mb-1">Mission Objective</span>
                <p className="text-slate-200 text-sm font-medium leading-relaxed">{reviewPackage.objective}</p>
              </div>

              {/* Verification & Telemetry Grid */}
              <div className="grid grid-cols-2 md:grid-cols-4 gap-3">
                <div className="p-3 bg-slate-950 rounded-lg border border-slate-800">
                  <span className="text-[10px] font-mono text-slate-400 uppercase tracking-wider block mb-1">Verification</span>
                  <div className="flex items-center space-x-1.5 font-medium">
                    {reviewPackage.verification.tests_passed && reviewPackage.verification.tree_clean && reviewPackage.verification.commit_exists ? (
                      <>
                        <CheckCircle2 className="w-4 h-4 text-emerald-400" />
                        <span className="text-emerald-400">PASSED</span>
                      </>
                    ) : (
                      <>
                        <XCircle className="w-4 h-4 text-rose-400" />
                        <span className="text-rose-400">FAILED</span>
                      </>
                    )}
                  </div>
                  <span className="text-[10px] text-slate-400 mt-1 block">
                    Tests: {reviewPackage.verification.tests_passed ? '0 exit code' : 'Failed'}
                  </span>
                </div>

                <div className="p-3 bg-slate-950 rounded-lg border border-slate-800">
                  <span className="text-[10px] font-mono text-slate-400 uppercase tracking-wider block mb-1">Deliverable Commit</span>
                  <div className="font-mono text-slate-200 text-xs truncate">
                    {reviewPackage.final_commit ? reviewPackage.final_commit.slice(0, 8) : 'None'}
                  </div>
                  <span className="text-[10px] text-slate-400 mt-1 block truncate">
                    Tree: {reviewPackage.verification.tree_clean ? 'Clean' : 'Dirty'}
                  </span>
                </div>

                <div className="p-3 bg-slate-950 rounded-lg border border-slate-800">
                  <span className="text-[10px] font-mono text-slate-400 uppercase tracking-wider block mb-1">Execution Metrics</span>
                  <div className="text-slate-200 font-mono text-xs">
                    {reviewPackage.total_executions} runs · {reviewPackage.recovery_attempts} recoveries
                  </div>
                  <span className="text-[10px] text-slate-400 mt-1 block">
                    Cycles: {reviewPackage.cycles_count}
                  </span>
                </div>

                <div className="p-3 bg-slate-950 rounded-lg border border-slate-800">
                  <span className="text-[10px] font-mono text-slate-400 uppercase tracking-wider block mb-1">Runtime</span>
                  <div className="text-slate-200 font-mono text-xs">
                    {reviewPackage.duration_secs}s
                  </div>
                  <span className="text-[10px] text-slate-400 mt-1 block truncate">
                    Agent: {reviewPackage.agent_used || 'gemini-cli'}
                  </span>
                </div>
              </div>

              {/* Changed Files & Diff Summary */}
              <div className="space-y-2">
                <div className="flex items-center justify-between">
                  <span className="text-xs font-semibold text-slate-300">
                    Changed Files ({reviewPackage.diff_summary.files_count})
                  </span>
                  <div className="flex items-center space-x-2 font-mono text-[11px]">
                    <span className="text-emerald-400">+{reviewPackage.diff_summary.insertions}</span>
                    <span className="text-rose-400">-{reviewPackage.diff_summary.deletions}</span>
                  </div>
                </div>
                {reviewPackage.files_changed && reviewPackage.files_changed.length > 0 ? (
                  <div className="p-2 bg-slate-950 rounded border border-slate-800 flex flex-wrap gap-1.5">
                    {reviewPackage.files_changed.map((file: string, idx: number) => (
                      <span key={idx} className="px-2 py-0.5 bg-slate-900 border border-slate-800 rounded text-slate-300 font-mono text-[11px]">
                        {file}
                      </span>
                    ))}
                  </div>
                ) : (
                  <p className="text-slate-400 text-xs italic">No changed files detected.</p>
                )}
              </div>

              {/* Git Diff Output */}
              <div className="space-y-1.5">
                <span className="text-xs font-semibold text-slate-300">Unified Deliverable Diff</span>
                <div className="bg-slate-950 p-3 rounded-lg border border-slate-800 font-mono text-[11px] text-slate-300 overflow-x-auto max-h-64 whitespace-pre">
                  {reviewPackage.full_diff || 'No diff output available.'}
                </div>
              </div>

              {/* Recent Audit Timeline */}
              {reviewPackage.audit_timeline && reviewPackage.audit_timeline.length > 0 && (
                <div className="space-y-1.5">
                  <span className="text-xs font-semibold text-slate-300">Audit Trail (Recent)</span>
                  <div className="bg-slate-950 p-2 rounded border border-slate-800 space-y-1 font-mono text-[10px]">
                    {reviewPackage.audit_timeline.slice(-5).map((evt: { timestamp: string; event_type: string; summary: string }, idx: number) => (
                      <div key={idx} className="flex items-center justify-between text-slate-400">
                        <div className="flex items-center space-x-2 truncate">
                          <span className="text-slate-300 font-semibold">{evt.event_type}</span>
                          <span className="truncate">{evt.summary}</span>
                        </div>
                        <span className="shrink-0 ml-2">{new Date(evt.timestamp).toLocaleTimeString()}</span>
                      </div>
                    ))}
                  </div>
                </div>
              )}
            </div>

            {/* Modal Footer */}
            <div className="p-4 border-t border-slate-800 bg-slate-950/50 flex items-center justify-between">
              <div>
                {reviewPackage.can_reject && (
                  <button
                    onClick={() => {
                      setShowReviewModal(false);
                      setShowRejectModal(true);
                    }}
                    disabled={actionLoading}
                    className="px-3 py-1.5 bg-rose-500/10 hover:bg-rose-500/20 text-rose-400 border border-rose-500/30 rounded text-xs font-medium transition-colors flex items-center space-x-1.5"
                  >
                    <X className="w-3.5 h-3.5" />
                    <span>Reject Deliverable...</span>
                  </button>
                )}
              </div>
              <div className="flex items-center space-x-2">
                <button
                  type="button"
                  onClick={() => setShowReviewModal(false)}
                  className="px-3 py-1.5 text-xs text-slate-400 hover:text-slate-200 transition-colors"
                >
                  Close
                </button>
                {reviewPackage.can_integrate && !reviewPackage.can_accept && (
                  <button
                    onClick={() => handleIntegrate(reviewPackage.mission_id)}
                    disabled={actionLoading}
                    className="px-4 py-1.5 bg-emerald-600 hover:bg-emerald-500 text-white rounded text-xs font-medium transition-colors flex items-center space-x-1.5 shadow"
                  >
                    <GitPullRequest className="w-3.5 h-3.5" />
                    <span>Integrate to {reviewPackage.target_branch}</span>
                  </button>
                )}
                {reviewPackage.can_accept && (
                  <>
                    <button
                      onClick={() => handleAccept(reviewPackage.mission_id, false)}
                      disabled={actionLoading}
                      className="px-3 py-1.5 bg-slate-800 hover:bg-slate-700 text-slate-200 border border-slate-700 rounded text-xs font-medium transition-colors"
                    >
                      Accept Only
                    </button>
                    <button
                      onClick={() => handleAccept(reviewPackage.mission_id, true)}
                      disabled={actionLoading}
                      className="px-4 py-1.5 bg-emerald-600 hover:bg-emerald-500 text-white rounded text-xs font-medium transition-colors flex items-center space-x-1.5 shadow"
                    >
                      <Check className="w-3.5 h-3.5" />
                      <span>Accept & Integrate</span>
                    </button>
                  </>
                )}
              </div>
            </div>
          </div>
        </div>
      )}

      {/* Reject Reason Modal */}
      {showRejectModal && selectedMission && (
        <div className="fixed inset-0 bg-black/80 z-50 flex items-center justify-center p-4 backdrop-blur-sm animate-fade-in">
          <div className="bg-slate-900 border border-slate-700 rounded-xl max-w-md w-full p-6 space-y-4 shadow-2xl">
            <div className="flex items-center space-x-2 text-rose-400">
              <AlertTriangle className="w-5 h-5" />
              <h3 className="font-semibold text-slate-100 text-sm">Reject Mission Deliverable</h3>
            </div>
            <p className="text-xs text-slate-400 leading-relaxed">
              Rejecting is non-destructive. Worktree artifacts, candidate commits, and verification logs remain intact in the audit trail.
            </p>
            <div className="space-y-1.5">
              <label className="block text-slate-300 text-xs font-medium">Rejection Reason (required)</label>
              <textarea
                value={rejectReason}
                onChange={(e) => setRejectReason(e.target.value)}
                placeholder="e.g. Solution did not meet architectural style guidelines..."
                className="w-full h-24 px-3 py-2 bg-slate-950 border border-slate-700 rounded text-xs text-slate-200 focus:outline-none focus:border-rose-500 resize-none font-sans"
              />
            </div>
            <div className="pt-1">
              <label className="flex items-center space-x-2 text-slate-300 text-xs cursor-pointer">
                <input
                  type="checkbox"
                  checked={replanOnReject}
                  onChange={(e) => setReplanOnReject(e.target.checked)}
                  className="rounded bg-slate-950 border-slate-700 text-indigo-500"
                />
                <span>Continue mission via Replanning with feedback</span>
              </label>
            </div>
            <div className="flex items-center justify-end space-x-2 pt-3 border-t border-slate-800">
              <button
                type="button"
                onClick={() => {
                  setShowRejectModal(false);
                  setRejectReason('');
                }}
                className="px-3 py-1.5 text-xs text-slate-400 hover:text-slate-200"
              >
                Cancel
              </button>
              <button
                onClick={() => handleRejectConfirm(selectedMission.id)}
                disabled={actionLoading || !rejectReason.trim()}
                className="px-4 py-1.5 bg-rose-600 hover:bg-rose-500 disabled:opacity-50 text-white rounded text-xs font-semibold transition-colors shadow"
              >
                {actionLoading ? 'Rejecting...' : 'Confirm Rejection'}
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
};
