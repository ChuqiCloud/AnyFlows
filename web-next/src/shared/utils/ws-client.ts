/**
 * WebSocket 客户端 - 实时事件推送
 * 替代 SSE，支持双向通信、更稳定的连接
 */

import { tokenManager } from "@/shared/auth/kernel";
import { tenantStorage } from "@/shared/tenant";
import { resolveApiBaseUrl } from "@/shared/utils/env";

// 事件类型
export type WSEventType =
  | "session.logout"
  | "session.revoked"
  | "session.expired"
  | "user.updated"
  | "payment.success"
  | "payment.failed"
  | "order.updated"
  | "heartbeat"
  | "connected"
  | "pong";

// 事件数据
export interface WSEvent {
  type: WSEventType;
  user_id?: string;
  session_id?: string;
  data?: any;
  timestamp?: number;
}

// 连接状态
export type WSConnectionState =
  | "connecting"
  | "connected"
  | "disconnected"
  | "error";

type WSEventListener = (event: WSEvent) => void;
type WSStateListener = (state: WSConnectionState) => void;

class WebSocketClient {
  private ws: WebSocket | null = null;
  private eventListeners: Map<WSEventType | "*", Set<WSEventListener>> =
    new Map();
  private stateListeners: Set<WSStateListener> = new Set();
  private state: WSConnectionState = "disconnected";
  private reconnectAttempts = 0;
  private maxReconnectAttempts = 10;
  private reconnectDelay = 1000;
  private reconnectTimer: ReturnType<typeof setTimeout> | null = null;
  private heartbeatTimer: ReturnType<typeof setInterval> | null = null;
  private connectPromise: Promise<void> | null = null;

  private readonly apiUrl: string;
  private readonly wsUrl: string;

  constructor() {
    // 与平台 HTTP 客户端共用同一寻址规则；WebSocket 构造器必须拿到绝对地址。
    const configuredApiUrl = resolveApiBaseUrl();
    this.apiUrl = configuredApiUrl || (typeof window !== "undefined" ? window.location.origin : "");
    this.wsUrl = this.apiUrl.replace(/^http/, "ws");
  }

  /**
   * 获取 WebSocket 一次性 token
   */
  private async getWSToken(): Promise<string | null> {
    const token = tokenManager.getAccessToken();

    if (!token) {
      return null;
    }

    try {
      const currentTenantId = tenantStorage.getCurrentTenantId();
      const response = await fetch(`${this.apiUrl}/ws/token`, {
        method: "POST",
        headers: {
          Authorization: `Bearer ${token}`,
          "Content-Type": "application/json",
          ...(currentTenantId ? { "X-Tenant-ID": currentTenantId } : {}),
        },
      });

      if (!response.ok) {
        return null;
      }

      const data = await response.json();

      return data.data?.token || null;
    } catch {
      return null;
    }
  }

  /**
   * 连接 WebSocket
   */
  async connect(): Promise<void> {
    if (this.connectPromise) {
      return this.connectPromise;
    }

    if (this.ws?.readyState === WebSocket.OPEN) {
      return;
    }

    if (!tokenManager.getAccessToken()) {
      return;
    }

    this.connectPromise = this.doConnect();

    try {
      await this.connectPromise;
    } finally {
      this.connectPromise = null;
    }
  }

  /**
   * 实际执行连接的内部方法
   */
  private async doConnect(): Promise<void> {
    this.setState("connecting");

    try {
      const wsToken = await this.getWSToken();

      if (!wsToken) {
        this.handleDisconnect();

        return;
      }

      this.ws = new WebSocket(
        `${this.wsUrl}/ws?token=${encodeURIComponent(wsToken)}`,
      );

      this.ws.onopen = () => {
        this.setState("connected");
        this.reconnectAttempts = 0;
        this.startHeartbeat();
      };

      this.ws.onmessage = (event) => {
        try {
          this.handleEvent(JSON.parse(event.data));
        } catch {
          // 忽略无法解析的服务端消息，避免中断连接
        }
      };

      this.ws.onclose = () => {
        this.stopHeartbeat();
        this.ws = null;
        this.handleDisconnect();
      };

      this.ws.onerror = () => {
        this.setState("error");
      };
    } catch {
      this.handleDisconnect();
    }
  }

  /**
   * 处理事件
   */
  private handleEvent(event: WSEvent): void {
    const listeners = this.eventListeners.get(event.type);

    if (listeners) {
      listeners.forEach((listener) => listener(event));
    }

    const allListeners = this.eventListeners.get("*");

    if (allListeners) {
      allListeners.forEach((listener) => listener(event));
    }
  }

  /**
   * 处理断开连接
   */
  private handleDisconnect(): void {
    this.setState("disconnected");

    if (!tokenManager.getAccessToken()) {
      return;
    }

    if (this.reconnectAttempts < this.maxReconnectAttempts) {
      this.reconnectAttempts++;

      const delay = Math.min(
        this.reconnectDelay * Math.pow(2, this.reconnectAttempts - 1),
        30000,
      );

      this.reconnectTimer = setTimeout(() => this.connect(), delay);

      return;
    }

    this.setState("error");
  }

  /**
   * 断开连接
   */
  disconnect(): void {
    if (this.reconnectTimer) {
      clearTimeout(this.reconnectTimer);
      this.reconnectTimer = null;
    }

    this.stopHeartbeat();

    if (this.ws) {
      this.ws.close();
      this.ws = null;
    }

    this.setState("disconnected");
    this.reconnectAttempts = 0;
  }

  /**
   * 发送消息
   */
  send(type: string, data?: any): void {
    if (this.ws?.readyState === WebSocket.OPEN) {
      this.ws.send(JSON.stringify({ type, data }));
    }
  }

  /**
   * 启动心跳
   */
  private startHeartbeat(): void {
    this.heartbeatTimer = setInterval(() => {
      this.send("ping");
    }, 30000);
  }

  /**
   * 停止心跳
   */
  private stopHeartbeat(): void {
    if (this.heartbeatTimer) {
      clearInterval(this.heartbeatTimer);
      this.heartbeatTimer = null;
    }
  }

  /**
   * 设置状态
   */
  private setState(state: WSConnectionState): void {
    this.state = state;
    this.stateListeners.forEach((listener) => listener(state));
  }

  /**
   * 获取当前状态
   */
  getState(): WSConnectionState {
    return this.state;
  }

  /**
   * 监听事件
   */
  on(eventType: WSEventType | "*", listener: WSEventListener): () => void {
    if (!this.eventListeners.has(eventType)) {
      this.eventListeners.set(eventType, new Set());
    }
    this.eventListeners.get(eventType)!.add(listener);

    return () => {
      this.eventListeners.get(eventType)?.delete(listener);
    };
  }

  /**
   * 监听状态变化
   */
  onStateChange(listener: WSStateListener): () => void {
    this.stateListeners.add(listener);

    return () => this.stateListeners.delete(listener);
  }

  /**
   * 移除监听
   */
  off(eventType: WSEventType, listener: WSEventListener): void {
    this.eventListeners.get(eventType)?.delete(listener);
  }
}

export const wsClient = new WebSocketClient();
export default wsClient;
