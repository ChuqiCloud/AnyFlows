import { Card } from '@/components/ui/card'
import { DashboardTrendBody, type DashboardTrendProps } from './dashboard-trend-body'

export function DashboardTrend(props: DashboardTrendProps) {
  return <Card className="report-trend overflow-hidden"><DashboardTrendBody {...props} /></Card>
}
