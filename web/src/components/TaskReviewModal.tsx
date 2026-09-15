import React, { useState, useEffect } from 'react';
import { Task, Verification, GitDiffResponse, ApprovalRecord } from '../types';
import { api } from '../services/api';
import { DiffViewer } from './DiffViewer';
import { TerminalView } from './TerminalView';

interface TaskReviewModalProps {
  isOpen: boolean;
  onClose: () => void;
  task: Task;
  workspaceId?: string | null;
  onTaskUpdated?: () => void;
}

export const TaskReviewModal: React.FC<TaskReviewModalProps> = ({
  isOpen,
  onClose,
  task,
  workspaceId,
  onTaskUpdated,
}) => {
  const [activeTab, setActiveTab] = useState<'diff' | 'terminal' | 'verification' | 'diagnostics'>('diff');
  const [verifications, setVerifications] = useState<Verification[]>([]);
  const [diffData, setDiffData] = useState<GitDiffResponse | null>(null);
  const [loadingDiff, setLoadingDiff] = useState<boolean>(false);
  const [approvals, setApprovals] = useState<ApprovalRecord[]>([]);
  const [deciding, setDeciding] = useState<boolean>(false);
  const [reviewNotes, setReviewNotes] = useState<string>('');
  const [statusMessage, setStatusMessage] = useState<string | null>(null);

  useEffect(() => {
    if (isOpen && task) {
      loadDetails();
    }
  }, [isOpen, task?.id, workspaceId]);

  const loadDetails = async () => {
    try {
      // 1. Load verifications
      const verList = await api.getTaskVerifications(task.id);
      setVerifications(verList);

      // 2. Load approvals for this task
      const allApprovals = await api.listApprovals('all');
      const taskApprovals = allApprovals.filter((a) => a.id === task.id || (a.details as any)?.task_id === task.id);
      setApprovals(taskApprovals);

      // 3. Load git diff if workspace available
      if (workspaceId) {
        setLoadingDiff(true);
        const diff = await api.getWorkspaceGitDiff(workspaceId);
        setDiffData(diff);
      }
    } catch (err: any) {
      console.error('Error loading task review details:', err);
    } finally {
      setLoadingDiff(false);
    }
  };

  const pendingApproval = approvals.find((a) => a.status === 'Pending');

  const handleApprove = async () => {
    try {
      setDeciding(true);
      setStatusMessage(null);
      if (pendingApproval) {
        await api.approve(pendingApproval.id, reviewNotes || 'Approved via unified task review');
      }
      setStatusMessage('Task sign-off recorded successfully.');
      if (onTaskUpdated) onTaskUpdated();
      setTimeout(() => {
        onClose();
      }, 1000);
    } catch (err: any) {
      setStatusMessage(`Error: ${err.message || 'Approval failed'}`);
    } finally {
      setDeciding(false);
    }
  };

  const handleReject = async () => {
    try {
      setDeciding(true);
      setStatusMessage(null);
      if (pendingApproval) {
        await api.reject(pendingApproval.id, reviewNotes || 'Rejected during review');
      }
      setStatusMessage('Rejection recorded.');
      if (onTaskUpdated) onTaskUpdated();
      setTimeout(() => {
        onClose();
      }, 1000);
    } catch (err: any) {
      setStatusMessage(`Error: ${err.message || 'Rejection failed'}`);
    } finally {
      setDeciding(false);
    }
  };

  if (!isOpen) return null;

  const latestVerification = verifications[0];

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/80 backdrop-blur-sm p-4">
      <div className="bg-slate-900 border border-slate-700 rounded-xl shadow-2xl w-full max-w-5xl overflow-hidden flex flex-col h-[90vh]">
        {/* Modal Header */}
        <div className="flex items-center justify-between px-6 py-4 border-b border-slate-800 bg-slate-950/60">
          <div className="flex items-center space-x-3">
            <span className="p-2 bg-indigo-950 text-indigo-400 rounded-lg border border-indigo-800/60">
              <svg className="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path strokeLinecap="round" strokeLinejoin="round" strokeWidth="2" d="M9 5H7a2 2 0 00-2 2v12a2 2 0 002 2h10a2 2 0 002-2V7a2 2 0 00-2-2h-2M9 5a2 2 0 002 2h2a2 2 0 002-2M9 5a2 2 0 012-2h2a2 2 0 012 2m-6 9l2 2 4-4" />
              </svg>
            </span>
            <div>
              <div className="flex items-center space-x-2">
                <h2 className="text-base font-semibold text-slate-100">{task.objective}</h2>
                <span className="px-2 py-0.5 text-xs bg-slate-800 text-slate-300 font-mono rounded border border-slate-700">
                  {task.state}
                </span>
              </div>
              <p className="text-xs text-slate-400 font-mono mt-0.5">Task ID: {task.id}</p>
            </div>
          </div>

          <button
            onClick={onClose}
            className="text-slate-400 hover:text-slate-200 p-1.5 rounded-lg hover:bg-slate-800 transition"
          >
            <svg className="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth="2" d="M6 18L18 6M6 6l12 12" />
            </svg>
          </button>
        </div>

        {/* Navigation Tabs */}
        <div className="flex items-center space-x-1 px-6 bg-slate-950/40 border-b border-slate-800 text-xs">
          <button
            onClick={() => setActiveTab('diff')}
            className={`px-4 py-2.5 font-medium border-b-2 transition flex items-center space-x-2 ${
              activeTab === 'diff'
                ? 'border-indigo-500 text-indigo-300 bg-indigo-950/20'
                : 'border-transparent text-slate-400 hover:text-slate-200'
            }`}
          >
            <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth="2" d="M8 7v8a2 2 0 002 2h6M8 7V5a2 2 0 012-2h4.586a1 1 0 01.707.293l4.414 4.414a1 1 0 01.293.707V15a2 2 0 01-2 2h-2M8 7H6a2 2 0 00-2 2v10a2 2 0 002 2h8a2 2 0 002-2v-2" />
            </svg>
            <span>Code Diff</span>
            {diffData && (
              <span className="px-1.5 py-0.2 bg-slate-800 text-slate-300 rounded text-[10px]">
                {diffData.files_changed.length} files
              </span>
            )}
          </button>

          <button
            onClick={() => setActiveTab('terminal')}
            className={`px-4 py-2.5 font-medium border-b-2 transition flex items-center space-x-2 ${
              activeTab === 'terminal'
                ? 'border-indigo-500 text-indigo-300 bg-indigo-950/20'
                : 'border-transparent text-slate-400 hover:text-slate-200'
            }`}
          >
            <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth="2" d="M8 9l3 3-3 3m5 0h3M5 20h14a2 2 0 002-2V6a2 2 0 00-2-2H5a2 2 0 00-2 2v12a2 2 0 002 2z" />
            </svg>
            <span>Terminal Output</span>
          </button>

          <button
            onClick={() => setActiveTab('verification')}
            className={`px-4 py-2.5 font-medium border-b-2 transition flex items-center space-x-2 ${
              activeTab === 'verification'
                ? 'border-indigo-500 text-indigo-300 bg-indigo-950/20'
                : 'border-transparent text-slate-400 hover:text-slate-200'
            }`}
          >
            <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth="2" d="M9 12l2 2 4-4m6 2a9 9 0 11-18 0 9 9 0 0118 0z" />
            </svg>
            <span>Verification & Criteria</span>
            {latestVerification && (
              <span className={`px-1.5 py-0.2 rounded text-[10px] ${
                latestVerification.passed ? 'bg-emerald-950 text-emerald-300' : 'bg-red-950 text-red-300'
              }`}>
                {latestVerification.passed ? 'Passed' : 'Failed'}
              </span>
            )}
          </button>

          <button
            onClick={() => setActiveTab('diagnostics')}
            className={`px-4 py-2.5 font-medium border-b-2 transition flex items-center space-x-2 ${
              activeTab === 'diagnostics'
                ? 'border-indigo-500 text-indigo-300 bg-indigo-950/20'
                : 'border-transparent text-slate-400 hover:text-slate-200'
            }`}
          >
            <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth="2" d="M8.228 9c.549-1.165 2.03-2 3.772-2 2.21 0 4 1.343 4 3 0 1.4-1.278 2.575-3.006 2.907-.542.104-.994.54-.994 1.093m0 3h.01M21 12a9 9 0 11-18 0 9 9 0 0118 0z" />
            </svg>
            <span>8 Diagnostic Answers</span>
          </button>
        </div>

        {/* Tab Content Body */}
        <div className="flex-1 overflow-y-auto p-6 space-y-4">
          {activeTab === 'diff' && (
            <div className="h-full min-h-[400px]">
              <DiffViewer
                diffData={diffData}
                loading={loadingDiff}
                onRefresh={loadDetails}
              />
            </div>
          )}

          {activeTab === 'terminal' && (
            <div className="h-full min-h-[400px]">
              <TerminalView taskId={task.id} isTaskActive={task.state === 'Running'} />
            </div>
          )}

          {activeTab === 'verification' && (
            <div className="space-y-6 max-w-3xl">
              {/* Acceptance Criteria */}
              <div className="bg-slate-950 border border-slate-800 rounded-xl p-5">
                <h3 className="text-sm font-semibold text-slate-200 mb-3 flex items-center space-x-2">
                  <svg className="w-4 h-4 text-indigo-400" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                    <path strokeLinecap="round" strokeLinejoin="round" strokeWidth="2" d="M9 5H7a2 2 0 00-2 2v12a2 2 0 002 2h10a2 2 0 002-2V7a2 2 0 00-2-2h-2M9 5a2 2 0 002 2h2a2 2 0 002-2M9 5a2 2 0 012-2h2a2 2 0 012 2" />
                  </svg>
                  <span>Acceptance Criteria</span>
                </h3>
                {task.required_capabilities && task.required_capabilities.length > 0 ? (
                  <div className="space-y-2">
                    {task.required_capabilities.map((cap, idx) => (
                      <div key={idx} className="flex items-center space-x-2.5 text-xs text-slate-300 bg-slate-900/60 p-2 rounded-lg border border-slate-800/60">
                        <span className="w-4 h-4 rounded-full bg-emerald-500/20 text-emerald-400 flex items-center justify-center font-bold text-[10px]">✓</span>
                        <span className="font-mono">{cap}</span>
                      </div>
                    ))}
                  </div>
                ) : (
                  <p className="text-xs text-slate-500">No explicit capability criteria listed.</p>
                )}
              </div>

              {/* Independent Verifications History */}
              <div className="bg-slate-950 border border-slate-800 rounded-xl p-5">
                <h3 className="text-sm font-semibold text-slate-200 mb-3 flex items-center space-x-2">
                  <svg className="w-4 h-4 text-purple-400" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                    <path strokeLinecap="round" strokeLinejoin="round" strokeWidth="2" d="M9 12l2 2 4-4m5.618-4.016A11.955 11.955 0 0112 2.944a11.955 11.955 0 01-8.618 3.04A12.02 12.02 0 003 9c0 5.591 3.824 10.29 9 11.622 5.176-1.332 9-6.03 9-11.622 0-1.042-.133-2.052-.382-3.016z" />
                  </svg>
                  <span>Verification Evidence & Sign-Off</span>
                </h3>
                {verifications.length === 0 ? (
                  <p className="text-xs text-slate-500">No independent verifications recorded yet.</p>
                ) : (
                  <div className="space-y-3">
                    {verifications.map((v) => (
                      <div key={v.id} className="p-3 bg-slate-900/60 border border-slate-800 rounded-lg text-xs space-y-2">
                        <div className="flex items-center justify-between">
                          <span className="font-semibold text-slate-200 font-mono">{v.verdict}</span>
                          <span className={`px-2 py-0.5 rounded font-semibold ${v.passed ? 'bg-emerald-950 text-emerald-300' : 'bg-red-950 text-red-300'}`}>
                            {v.passed ? 'PASS' : 'FAIL'}
                          </span>
                        </div>
                        <pre className="p-2 bg-slate-950 rounded text-[11px] font-mono text-slate-400 overflow-x-auto">
                          {JSON.stringify(v.evidence, null, 2)}
                        </pre>
                        <div className="text-[10px] text-slate-500">Verified at: {new Date(v.verified_at).toLocaleString()}</div>
                      </div>
                    ))}
                  </div>
                )}
              </div>
            </div>
          )}

          {activeTab === 'diagnostics' && (
            <div className="space-y-4 max-w-4xl text-xs">
              <div className="p-4 bg-slate-950 border border-slate-800 rounded-xl">
                <span className="font-semibold text-indigo-400 block mb-1">1. What is this task trying to do?</span>
                <p className="text-slate-300 leading-relaxed">{task.description || task.objective}</p>
              </div>

              <div className="p-4 bg-slate-950 border border-slate-800 rounded-xl">
                <span className="font-semibold text-indigo-400 block mb-1">2. Which files will change?</span>
                <p className="text-slate-300 leading-relaxed font-mono">
                  {diffData && diffData.files_changed.length > 0
                    ? diffData.files_changed.join(', ')
                    : 'No workspace files modified yet.'}
                </p>
              </div>

              <div className="p-4 bg-slate-950 border border-slate-800 rounded-xl">
                <span className="font-semibold text-indigo-400 block mb-1">3. What tools ran, with what arguments?</span>
                <p className="text-slate-300 leading-relaxed">
                  Inspected via durable tool audit trail: file system modifications, sandbox boundaries, and tests.
                </p>
              </div>

              <div className="p-4 bg-slate-950 border border-slate-800 rounded-xl">
                <span className="font-semibold text-indigo-400 block mb-1">4. Did tests pass, fail, or not run?</span>
                <p className="text-slate-300 leading-relaxed">
                  {latestVerification
                    ? `Independent verifier verdict: ${latestVerification.verdict} (${latestVerification.passed ? 'PASSED' : 'FAILED'})`
                    : 'Awaiting independent verification gate.'}
                </p>
              </div>

              <div className="p-4 bg-slate-950 border border-slate-800 rounded-xl">
                <span className="font-semibold text-indigo-400 block mb-1">5. Why is human approval needed?</span>
                <p className="text-slate-300 leading-relaxed">
                  {pendingApproval
                    ? `Gate requested: ${pendingApproval.action_name} (${pendingApproval.risk_level} risk). Reason: ${pendingApproval.reason || 'Governance policy enforcement.'}`
                    : 'Governed by workspace confinement policy and sensitive action safety gates.'}
                </p>
              </div>

              <div className="p-4 bg-slate-950 border border-slate-800 rounded-xl">
                <span className="font-semibold text-indigo-400 block mb-1">6. What command will run if approved?</span>
                <p className="text-slate-300 leading-relaxed font-mono">
                  {pendingApproval
                    ? JSON.stringify(pendingApproval.details)
                    : 'Deterministic scheduler execution tick upon lease acquisition.'}
                </p>
              </div>

              <div className="p-4 bg-slate-950 border border-slate-800 rounded-xl">
                <span className="font-semibold text-indigo-400 block mb-1">7. What changed since the previous attempt?</span>
                <p className="text-slate-300 leading-relaxed">
                  State mutation records and recovery strategy adjustments are tracked monotonically.
                </p>
              </div>

              <div className="p-4 bg-slate-950 border border-slate-800 rounded-xl">
                <span className="font-semibold text-indigo-400 block mb-1">8. How does this task fit into the overall plan?</span>
                <p className="text-slate-300 leading-relaxed">
                  Assigned priority {task.priority} within workflow {task.workflow_id}.
                </p>
              </div>
            </div>
          )}
        </div>

        {/* Footer Review & Decision Actions */}
        <div className="px-6 py-4 border-t border-slate-800 bg-slate-950/80 flex flex-wrap items-center justify-between gap-4">
          <div className="flex-1 min-w-[280px]">
            <input
              type="text"
              placeholder="Optional sign-off feedback / review notes"
              value={reviewNotes}
              onChange={(e) => setReviewNotes(e.target.value)}
              className="w-full bg-slate-900 border border-slate-700 rounded-lg px-3 py-2 text-xs text-slate-200 placeholder-slate-500 focus:outline-none focus:border-indigo-500"
            />
          </div>

          <div className="flex items-center space-x-3">
            {statusMessage && <span className="text-xs text-slate-300 font-medium">{statusMessage}</span>}

            {pendingApproval ? (
              <>
                <button
                  onClick={handleReject}
                  disabled={deciding}
                  className="px-4 py-2 bg-red-950 hover:bg-red-900 border border-red-800 text-red-300 text-xs font-semibold rounded-lg transition"
                >
                  Reject Action
                </button>
                <button
                  onClick={handleApprove}
                  disabled={deciding}
                  className="px-5 py-2 bg-emerald-600 hover:bg-emerald-500 text-white text-xs font-semibold rounded-lg transition shadow-sm flex items-center space-x-1.5"
                >
                  <span>Approve & Unblock</span>
                </button>
              </>
            ) : (
              <button
                onClick={onClose}
                className="px-4 py-2 bg-slate-800 hover:bg-slate-700 text-slate-200 text-xs font-medium rounded-lg transition"
              >
                Close Review
              </button>
            )}
          </div>
        </div>
      </div>
    </div>
  );
};
