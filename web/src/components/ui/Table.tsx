import React from 'react';

export interface TableProps extends React.TableHTMLAttributes<HTMLTableElement> {
  containerClassName?: string;
}

export const Table: React.FC<TableProps> = ({
  children,
  className = '',
  containerClassName = '',
  ...props
}) => (
  <div className={`w-full overflow-x-auto ${containerClassName}`}>
    <table
      className={`w-full text-left text-xs border-collapse ${className}`}
      {...props}
    >
      {children}
    </table>
  </div>
);

export const Thead: React.FC<React.HTMLAttributes<HTMLTableSectionElement>> = ({
  children,
  className = '',
  ...props
}) => (
  <thead
    className={`bg-surface-header/80 text-gray-400 font-mono text-[11px] uppercase tracking-wider border-b border-surface-border ${className}`}
    {...props}
  >
    {children}
  </thead>
);

export const Tbody: React.FC<React.HTMLAttributes<HTMLTableSectionElement>> = ({
  children,
  className = '',
  ...props
}) => (
  <tbody className={`divide-y divide-surface-border ${className}`} {...props}>
    {children}
  </tbody>
);

export interface TrProps extends React.HTMLAttributes<HTMLTableRowElement> {
  isInteractive?: boolean;
}

export const Tr: React.FC<TrProps> = ({
  children,
  className = '',
  isInteractive = false,
  ...props
}) => (
  <tr
    className={`transition-colors ${
      isInteractive ? 'hover:bg-surface-hover/60 cursor-pointer' : 'hover:bg-surface-hover/30'
    } ${className}`}
    {...props}
  >
    {children}
  </tr>
);

export const Th: React.FC<React.ThHTMLAttributes<HTMLTableCellElement>> = ({
  children,
  className = '',
  ...props
}) => (
  <th
    className={`px-3.5 py-2.5 font-medium whitespace-nowrap ${className}`}
    {...props}
  >
    {children}
  </th>
);

export interface TdProps extends React.TdHTMLAttributes<HTMLTableCellElement> {
  mono?: boolean;
}

export const Td: React.FC<TdProps> = ({
  children,
  className = '',
  mono = false,
  ...props
}) => (
  <td
    className={`px-3.5 py-2.5 text-gray-300 ${mono ? 'font-mono text-[11px]' : ''} ${className}`}
    {...props}
  >
    {children}
  </td>
);
