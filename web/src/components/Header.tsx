import React from 'react';
import {
  Activity,
  Layers,
  ShieldCheck,
  Radio,
  Bot,
  Database,
  Wrench,
  Server,
  Plus,
  Settings,
  RefreshCw,
  Folder,
  GitBranch,
  DollarSign,
  Target,
} from 'lucide-react';
import { ConnectionState } from '../services/sse';
import { Workspace } from '../types';
import { Button } from './ui/Button';
import { StatusDot } from './ui/StatusDot';

export type TabType =
  | 'dashboard'
  | 'missions'
  | 'workflows'
  | 'approvals'
  | 'timeline'
  | 'agents'
  | 'memory'
  | 'tools'
  | 'providers'
  | 'usage';

interface HeaderProps {
  activeTab: TabType;
  onSelectTab: (tab: TabType) => void;
  connectionState: ConnectionState;
  cursor: number;
  pendingApprovalsCount: number;
  activeWorkspace?: Workspace | null;
  onOpenWorkspaceModal?: () => void;
  onOpenNewWorkflow: () => void;
  onOpenSettings: () => void;
  onRefresh: () => void;
}

export const Header: React.FC<HeaderProps> = ({
  activeTab,
  onSelectTab,
  connectionState,
  cursor,
  pendingApprovalsCount,
  activeWorkspace,
  onOpenWorkspaceModal,
  onOpenNewWorkflow,
  onOpenSettings,
  onRefresh,
}) => {
  const tabs: Array<{ id: TabType; label: string; icon: React.ReactNode; badge?: number }> = [
    { id: 'missions', label: 'Missions', icon: <Target className="w-3.5 h-3.5" /> },
    { id: 'dashboard', label: 'Dashboard', icon: <Activity className="w-3.5 h-3.5" /> },
    { id: 'workflows', label: 'Workflows', icon: <Layers className="w-3.5 h-3.5" /> },
    {
      id: 'approvals',
      label: 'Approvals',
      icon: <ShieldCheck className="w-3.5 h-3.5" />,
      badge: pendingApprovalsCount,
    },
    { id: 'timeline', label: 'Live Timeline', icon: <Radio className="w-3.5 h-3.5" /> },
    { id: 'agents', label: 'Agents', icon: <Bot className="w-3.5 h-3.5" /> },
    { id: 'memory', label: 'Memory', icon: <Database className="w-3.5 h-3.5" /> },
    { id: 'tools', label: 'Tools', icon: <Wrench className="w-3.5 h-3.5" /> },
    { id: 'providers', label: 'Providers', icon: <Server className="w-3.5 h-3.5" /> },
    { id: 'usage', label: 'Usage & Retention', icon: <DollarSign className="w-3.5 h-3.5" /> },
  ];

  const dotStatus =
    connectionState === 'connected'
      ? 'verified'
      : connectionState === 'reconnecting'
      ? 'awaiting'
      : 'failed';

  return (
    <header className="bg-surface-header border-b border-surface-border sticky top-0 z-40 px-3 sm:px-5 py-2">
      <div className="max-w-7xl mx-auto flex items-center justify-between gap-4">
        {/* Brand & Navigation */}
        <div className="flex items-center gap-6 min-w-0">
          {/* Brand Anchor */}
          <div
            className="flex items-center gap-2.5 cursor-pointer shrink-0 group select-none"
            onClick={() => onSelectTab('missions')}
            title="Axonel Control Plane"
          >
            <div className="w-7 h-7 rounded bg-surface-base border border-surface-border-bold flex items-center justify-center transition-colors group-hover:border-axonel-lime">
              <span className="font-mono font-bold text-axonel-lime text-base leading-none">
                A
              </span>
            </div>
            <div className="leading-tight hidden sm:block">
              <div className="flex items-center gap-1.5">
                <span className="font-bold text-gray-100 tracking-tight text-xs font-mono">
                  AXONEL
                </span>
                <span className="text-[9px] font-mono uppercase tracking-widest px-1 py-0.2 rounded bg-surface-card border border-surface-border text-gray-400">
                  v0.1.1
                </span>
              </div>
              <p className="text-[10px] text-gray-400 font-mono tracking-tight">
                control plane
              </p>
            </div>
          </div>

          {/* Navigation Tabs */}
          <nav className="hidden lg:flex items-center gap-1">
            {tabs.map((tab) => {
              const isActive = activeTab === tab.id;
              return (
                <button
                  key={tab.id}
                  onClick={() => onSelectTab(tab.id)}
                  className={`flex items-center gap-1.5 px-2.5 py-1.5 rounded text-xs font-medium transition-colors select-none ${
                    isActive
                      ? 'bg-surface-card text-gray-100 border border-surface-border shadow-xs'
                      : 'text-gray-400 hover:text-gray-200 hover:bg-surface-hover/60 border border-transparent'
                  }`}
                >
                  <span className={isActive ? 'text-axonel-lime' : 'text-gray-400'}>
                    {tab.icon}
                  </span>
                  <span>{tab.label}</span>
                  {tab.badge && tab.badge > 0 ? (
                    <span className="ml-0.5 px-1.5 py-0.2 bg-amber-950/60 text-status-needshuman border border-amber-600/70 rounded-full text-[10px] font-mono font-bold">
                      {tab.badge}
                    </span>
                  ) : null}
                </button>
              );
            })}
          </nav>
        </div>

        {/* Right Controls */}
        <div className="flex items-center gap-2 sm:gap-2.5 shrink-0">
          {/* Workspace Switcher */}
          <button
            onClick={onOpenWorkspaceModal}
            className="flex items-center gap-1.5 px-2.5 py-1 bg-surface-card hover:bg-surface-hover text-gray-200 border border-surface-border rounded text-xs font-mono transition-colors"
            title="Switch project workspace"
          >
            <Folder className="w-3.5 h-3.5 text-gray-400" />
            <span className="font-semibold max-w-[110px] truncate">
              {activeWorkspace?.name || 'Workspace'}
            </span>
            {activeWorkspace?.vcs?.branch && (
              <span className="text-[10px] text-emerald-400 flex items-center gap-0.5 ml-0.5">
                <GitBranch className="w-2.5 h-2.5" />
                <span className="max-w-[70px] truncate">{activeWorkspace.vcs.branch}</span>
              </span>
            )}
          </button>

          {/* Daemon Connection Indicator */}
          <div
            className="hidden sm:flex items-center gap-2 px-2.5 py-1 rounded bg-surface-base border border-surface-border text-xs font-mono"
            title={`Authoritative SSE Stream: ${connectionState}, Sequence #${cursor}`}
          >
            <StatusDot
              status={dotStatus}
              pulse={connectionState === 'connected' || connectionState === 'reconnecting'}
              size="xs"
            />
            <span className="capitalize text-gray-300 text-[11px]">{connectionState}</span>
            <span className="text-surface-border-bold text-[10px]">|</span>
            <span className="text-gray-400 text-[11px]">#{cursor}</span>
          </div>

          {/* Refresh Action */}
          <Button
            variant="outline"
            size="sm"
            onClick={onRefresh}
            title="Refresh state"
            aria-label="Refresh state"
            className="px-2 py-1"
          >
            <RefreshCw className="w-3.5 h-3.5 text-gray-400" />
          </Button>

          {/* New Workflow Action */}
          <Button
            variant="primary"
            size="sm"
            onClick={onOpenNewWorkflow}
            icon={<Plus className="w-3.5 h-3.5" />}
          >
            <span className="hidden sm:inline">New Workflow</span>
            <span className="sm:hidden">New</span>
          </Button>

          {/* Settings */}
          <Button
            variant="outline"
            size="sm"
            onClick={onOpenSettings}
            title="Settings & Auth"
            aria-label="Settings & Auth"
            className="px-2 py-1"
          >
            <Settings className="w-3.5 h-3.5 text-gray-400" />
          </Button>
        </div>
      </div>

      {/* Mobile navigation row */}
      <div className="lg:hidden flex items-center gap-1 overflow-x-auto py-2 border-t border-surface-border mt-2 -mx-3 px-3">
        {tabs.map((tab) => {
          const isActive = activeTab === tab.id;
          return (
            <button
              key={tab.id}
              onClick={() => onSelectTab(tab.id)}
              className={`flex items-center gap-1.5 px-2.5 py-1 rounded text-xs font-medium whitespace-nowrap transition-colors select-none ${
                isActive
                  ? 'bg-surface-card text-gray-100 border border-surface-border shadow-xs'
                  : 'text-gray-400 hover:text-gray-200 hover:bg-surface-hover/60 border border-transparent'
              }`}
            >
              <span className={isActive ? 'text-axonel-lime' : 'text-gray-400'}>
                {tab.icon}
              </span>
              <span>{tab.label}</span>
              {tab.badge && tab.badge > 0 ? (
                <span className="ml-0.5 px-1.5 py-0.2 bg-amber-950/60 text-status-needshuman border border-amber-600/70 rounded-full text-[10px] font-mono font-bold">
                  {tab.badge}
                </span>
              ) : null}
            </button>
          );
        })}
      </div>
    </header>
  );
};
