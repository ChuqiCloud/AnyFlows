import type { NotificationItem, NotificationTab } from "../types";

/**
 * Mock 通知数据（后续替换为真实 API）
 */
export const mockNotifications: Record<NotificationTab, NotificationItem[]> = {
  all: [
    {
      id: "1",
      isRead: false,
      icon: "solar:user-plus-bold",
      title: "新成员申请",
      description: "张三 申请加入您的团队",
      time: "2 小时前",
      type: "request",
    },
    {
      id: "2",
      isRead: false,
      icon: "solar:server-bold",
      title: "服务器告警",
      description: "生产环境 CPU 使用率超过 80%",
      time: "5 小时前",
      type: "system",
    },
    {
      id: "3",
      isRead: false,
      icon: "solar:bill-check-bold",
      title: "账单提醒",
      description: "您有一笔待支付账单，金额 ¥128.00",
      time: "昨天",
      type: "default",
    },
    {
      id: "4",
      isRead: true,
      icon: "solar:shield-check-bold",
      title: "安全提醒",
      description: "检测到新设备登录，请确认是否为本人操作",
      time: "昨天",
      type: "system",
    },
    {
      id: "5",
      isRead: true,
      icon: "solar:document-bold",
      title: "工单回复",
      description: "您的工单 #2024010001 已处理完成",
      time: "2 天前",
      type: "default",
    },
  ],
  unread: [
    {
      id: "1",
      isRead: false,
      icon: "solar:user-plus-bold",
      title: "新成员申请",
      description: "张三 申请加入您的团队",
      time: "2 小时前",
      type: "request",
    },
    {
      id: "2",
      isRead: false,
      icon: "solar:server-bold",
      title: "服务器告警",
      description: "生产环境 CPU 使用率超过 80%",
      time: "5 小时前",
      type: "system",
    },
    {
      id: "3",
      isRead: false,
      icon: "solar:bill-check-bold",
      title: "账单提醒",
      description: "您有一笔待支付账单，金额 ¥128.00",
      time: "昨天",
      type: "default",
    },
  ],
  archive: [],
};

/**
 * 获取未读通知数量
 */
export const getUnreadCount = (): number => {
  return mockNotifications.unread.length;
};
