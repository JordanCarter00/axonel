import { EventRecord } from '../types';

export type ConnectionState = 'connected' | 'connecting' | 'reconnecting' | 'disconnected';

export type EventCallback = (event: EventRecord) => void;
export type StateCallback = (state: ConnectionState, cursor: number) => void;

class EventStreamManager {
  private eventSource: EventSource | null = null;
  private lastCursor: number = 0;
  private connectionState: ConnectionState = 'disconnected';
  private listeners: Map<string, Set<EventCallback>> = new Map();
  private wildcardListeners: Set<EventCallback> = new Set();
  private stateListeners: Set<StateCallback> = new Set();
  private reconnectTimeout: number | null = null;
  private reconnectAttempts: number = 0;
  private shouldBeConnected: boolean = false;

  constructor() {
    const savedCursor = sessionStorage.getItem('plexis_event_cursor');
    if (savedCursor) {
      const parsed = parseInt(savedCursor, 10);
      if (!isNaN(parsed)) {
        this.lastCursor = parsed;
      }
    }
  }

  public getCursor(): number {
    return this.lastCursor;
  }

  public getConnectionState(): ConnectionState {
    return this.connectionState;
  }

  public onStateChange(callback: StateCallback): () => void {
    this.stateListeners.add(callback);
    callback(this.connectionState, this.lastCursor);
    return () => {
      this.stateListeners.delete(callback);
    };
  }

  private setState(state: ConnectionState) {
    this.connectionState = state;
    for (const listener of this.stateListeners) {
      try {
        listener(this.connectionState, this.lastCursor);
      } catch (err) {
        console.error('Error in state listener:', err);
      }
    }
  }

  public connect() {
    this.shouldBeConnected = true;
    this.initiateConnection();
  }

  private initiateConnection() {
    if (this.eventSource) {
      this.eventSource.close();
      this.eventSource = null;
    }

    if (this.reconnectTimeout) {
      window.clearTimeout(this.reconnectTimeout);
      this.reconnectTimeout = null;
    }

    this.setState(this.reconnectAttempts > 0 ? 'reconnecting' : 'connecting');

    const url = `/api/v1/events/stream?after=${this.lastCursor}`;
    try {
      this.eventSource = new EventSource(url);

      this.eventSource.onopen = () => {
        this.reconnectAttempts = 0;
        this.setState('connected');
      };

      this.eventSource.onmessage = (messageEvent) => {
        try {
          const record: EventRecord = JSON.parse(messageEvent.data);
          if (record && typeof record.sequence === 'number') {
            if (record.sequence > this.lastCursor) {
              this.lastCursor = record.sequence;
              sessionStorage.setItem('plexis_event_cursor', this.lastCursor.toString());
              this.notifyState();
            }
          }
          this.dispatchEvent(record);
        } catch (e) {
          console.warn('Failed to parse incoming SSE message:', messageEvent.data, e);
        }
      };

      this.eventSource.onerror = () => {
        if (!this.shouldBeConnected) return;

        if (this.eventSource) {
          this.eventSource.close();
          this.eventSource = null;
        }

        this.setState('reconnecting');
        this.scheduleReconnect();
      };
    } catch (err) {
      console.error('Failed to create EventSource:', err);
      this.scheduleReconnect();
    }
  }

  private scheduleReconnect() {
    if (!this.shouldBeConnected) return;

    this.reconnectAttempts++;
    const delay = Math.min(1000 * Math.pow(1.5, this.reconnectAttempts - 1), 10000);
    this.reconnectTimeout = window.setTimeout(() => {
      this.initiateConnection();
    }, delay);
  }

  public disconnect() {
    this.shouldBeConnected = false;
    if (this.reconnectTimeout) {
      window.clearTimeout(this.reconnectTimeout);
      this.reconnectTimeout = null;
    }
    if (this.eventSource) {
      this.eventSource.close();
      this.eventSource = null;
    }
    this.setState('disconnected');
  }

  public subscribe(eventType: string, callback: EventCallback): () => void {
    if (!this.listeners.has(eventType)) {
      this.listeners.set(eventType, new Set());
    }
    this.listeners.get(eventType)!.add(callback);

    return () => {
      this.listeners.get(eventType)?.delete(callback);
    };
  }

  public subscribeAll(callback: EventCallback): () => void {
    this.wildcardListeners.add(callback);
    return () => {
      this.wildcardListeners.delete(callback);
    };
  }

  private dispatchEvent(record: EventRecord) {
    for (const listener of this.wildcardListeners) {
      try {
        listener(record);
      } catch (err) {
        console.error('Wildcard event listener threw error:', err);
      }
    }

    const typeListeners = this.listeners.get(record.event_type);
    if (typeListeners) {
      for (const listener of typeListeners) {
        try {
          listener(record);
        } catch (err) {
          console.error(`Event listener for ${record.event_type} threw error:`, err);
        }
      }
    }
  }

  private notifyState() {
    for (const listener of this.stateListeners) {
      try {
        listener(this.connectionState, this.lastCursor);
      } catch (err) {
        console.error('Error in state listener:', err);
      }
    }
  }
}

export const eventStream = new EventStreamManager();
