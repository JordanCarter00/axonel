import React, { useState, useEffect, useCallback } from 'react';
import { api } from './services/api';
import { eventStream, ConnectionState } from './services/sse';
import { Header, TabType } from './components/Header';
import { DashboardView } from './components/DashboardView';
import { WorkflowsView } from './components/WorkflowsView';
import { WorkflowDetailView } from './components/WorkflowDetailView';
import { ApprovalsView } from './components/ApprovalsView';
import { LiveTimelineView } from './components/LiveTimelineView';
import { AgentsView } from './components/AgentsView';
import { ToolsView } from './components/ToolsView';
import { MemoryView } from './components/MemoryView';
import { ProvidersView } from './components/ProvidersView';
import { NewWorkflowModal } from './components/NewWorkflowModal';
import { SettingsModal } from './components/SettingsModal';
import { DashboardSummary, Workflow, Agent, ApprovalRecord } from './types';

export const App: React.FC = () => {
  const [activeTab, setActiveTab] = useState<TabType>(() => {
    try {
      const saved = sessionStorage.getItem('plexis_active_tab') as TabType;
      return saved || 'dashboard';
    } catch {
      return 'dashboard';
    }
  });
  const [selectedWorkflowId, setSelectedWorkflowId] = useState<string | null>(() => {
    try {
      return sessionStorage.getItem('plexis_active_workflow_id');
    } catch {
      return null;
    }
  });

  const [dashboardSummary, setDashboardSummary] = useState<DashboardSummary | null>(null);
  const [workflows, setWorkflows] = useState<Workflow[]>([]);
  const [agents, setAgents] = useState<Agent[]>([]);
  const [approvals, setApprovals] = useState<ApprovalRecord[]>([]);

  const [connectionState, setConnectionState] = useState<ConnectionState>(
    eventStream.getConnectionState()
  );
  const [cursor, setCursor] = useState<number>(eventStream.getCursor());

  const [isNewWorkflowOpen, setIsNewWorkflowOpen] = useState(false);
  const [isSettingsOpen, setIsSettingsOpen] = useState(false);
  const [loading, setLoading] = useState(true);

  const loadData = useCallback(async () => {
    try {
      const [sum, wfs, ags, apps] = await Promise.all([
        api.getDashboardSummary().catch(() => null),
        api.listWorkflows().catch(() => []),
        api.listAgents().catch(() => []),
        api.listApprovals('all').catch(() => []),
      ]);
      if (sum) setDashboardSummary(sum);
      setWorkflows(wfs);
      setAgents(ags);
      setApprovals(apps);
    } catch (e) {
      console.error('Error fetching dashboard state:', e);
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    loadData();

    // Connect to live SSE event stream
    eventStream.connect();

    const unsubState = eventStream.onStateChange((state, cur) => {
      setConnectionState(state);
      setCursor(cur);
    });

    const unsubEvents = eventStream.subscribeAll(() => {
      // Whenever an authoritative event occurs, update state
      loadData();
    });

    return () => {
      unsubState();
      unsubEvents();
      eventStream.disconnect();
    };
  }, [loadData]);

  const pendingApprovalsCount = approvals.filter((a) => {
    const s = (a.status || (a as unknown as { state?: string }).state || '').toLowerCase();
    return s === 'pending';
  }).length;

  const handleSelectWorkflow = (id: string) => {
    setSelectedWorkflowId(id);
    try {
      sessionStorage.setItem('plexis_active_workflow_id', id);
    } catch {}
  };

  const handleTabSelect = (tab: TabType) => {
    setActiveTab(tab);
    setSelectedWorkflowId(null);
    try {
      sessionStorage.setItem('plexis_active_tab', tab);
      sessionStorage.removeItem('plexis_active_workflow_id');
    } catch {}
    loadData();
  };

  return (
    <div className="min-h-screen bg-background text-slate-100 flex flex-col font-sans">
      <Header
        activeTab={activeTab}
        onSelectTab={handleTabSelect}
        connectionState={connectionState}
        cursor={cursor}
        pendingApprovalsCount={pendingApprovalsCount}
        onOpenNewWorkflow={() => setIsNewWorkflowOpen(true)}
        onOpenSettings={() => setIsSettingsOpen(true)}
        onRefresh={loadData}
      />

      <main className="flex-1 max-w-7xl w-full mx-auto p-4 sm:p-6">
        {selectedWorkflowId ? (
          <WorkflowDetailView
            workflowId={selectedWorkflowId}
            onBack={() => setSelectedWorkflowId(null)}
            availableAgents={agents}
          />
        ) : (
          <>
            {activeTab === 'dashboard' && (
              <DashboardView
                summary={dashboardSummary}
                loading={loading}
                onSelectWorkflow={handleSelectWorkflow}
                onOpenApprovals={() => setActiveTab('approvals')}
                onOpenNewWorkflow={() => setIsNewWorkflowOpen(true)}
                onRefresh={loadData}
              />
            )}

            {activeTab === 'workflows' && (
              <WorkflowsView
                workflows={workflows}
                loading={loading}
                onSelectWorkflow={handleSelectWorkflow}
                onOpenNewWorkflow={() => setIsNewWorkflowOpen(true)}
              />
            )}

            {activeTab === 'approvals' && (
              <ApprovalsView
                approvals={approvals}
                loading={loading}
                onRefresh={loadData}
                onSelectWorkflow={handleSelectWorkflow}
              />
            )}

            {activeTab === 'timeline' && (
              <LiveTimelineView onSelectWorkflow={handleSelectWorkflow} />
            )}

            {activeTab === 'agents' && (
              <AgentsView
                agents={agents}
                loading={loading}
                onRefresh={loadData}
                onSelectWorkflow={handleSelectWorkflow}
              />
            )}

            {activeTab === 'memory' && <MemoryView />}

            {activeTab === 'tools' && <ToolsView />}

            {activeTab === 'providers' && <ProvidersView />}
          </>
        )}
      </main>

      <NewWorkflowModal
        isOpen={isNewWorkflowOpen}
        onClose={() => setIsNewWorkflowOpen(false)}
        onCreated={(id) => {
          loadData();
          setSelectedWorkflowId(id);
        }}
      />

      <SettingsModal
        isOpen={isSettingsOpen}
        onClose={() => setIsSettingsOpen(false)}
        onTokenUpdated={loadData}
      />
    </div>
  );
};
export default App;
