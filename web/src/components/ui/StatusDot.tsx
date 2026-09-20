import React from 'react';

export type StatusType =
  | 'running'
  | 'verified'
  | 'awaiting'
  | 'accepted'
  | 'integrating'
  | 'integrated'
  | 'failed'
  | 'rejected'
  | 'needshuman'
  | 'pending'
  | 'cancelled';

export interface StatusDotProps {
  status: StatusType | string;
  pulse?: boolean;
  className?: string;
  size?: 'xs' | 'sm' | 'md';
}

export const StatusDot: React.FC<StatusDotProps> = ({
  status,
  pulse = false,
  className = '',
  size = 'sm',
}) => {
  const norm = status.toLowerCase();

  const colorMap: Record<string, string> = {
    running: 'bg-status-running',
    verifying: 'bg-status-running',
    verified: 'bg-status-verified',
    awaiting: 'bg-status-awaiting',
    awaiting_acceptance: 'bg-status-awaiting',
    accepted: 'bg-status-accepted',
    integrating: 'bg-status-integrating',
    integrated: 'bg-status-integrated',
    completed: 'bg-status-verified',
    failed: 'bg-status-failed',
    rejected: 'bg-status-rejected',
    needshuman: 'bg-status-needshuman',
    needs_human: 'bg-status-needshuman',
    pending: 'bg-status-pending',
    created: 'bg-status-pending',
    cancelled: 'bg-status-cancelled',
  };

  const color = colorMap[norm] || 'bg-gray-500';

  const sizeClasses = {
    xs: 'w-1.5 h-1.5',
    sm: 'w-2 h-2',
    md: 'w-2.5 h-2.5',
  };

  return (
    <span className={`relative inline-flex shrink-0 ${sizeClasses[size]} ${className}`}>
      {pulse && (
        <span
          className={`absolute inline-flex h-full w-full rounded-full opacity-75 animate-ping ${color}`}
        />
      )}
      <span className={`relative inline-flex rounded-full ${sizeClasses[size]} ${color}`} />
    </span>
  );
};
