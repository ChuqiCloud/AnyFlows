import { Card } from '@heroui/react'
import { DashboardTrendBody, type DashboardTrendProps } from './dashboard-trend-body'

export function DashboardTrend(props: DashboardTrendProps) {
  return <Card className="report-trend overflow-hidden border border-[var(--hairline)] bg-card" shadow="none"><DashboardTrendBody {...props} /></Card>
}
