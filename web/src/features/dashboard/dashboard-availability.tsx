import { Card } from '@/components/ui/card'
import { DashboardAvailabilityBody, type DashboardAvailabilityProps } from './dashboard-availability-body'

export function DashboardAvailability(props: DashboardAvailabilityProps) {
  return <Card><DashboardAvailabilityBody {...props} /></Card>
}
