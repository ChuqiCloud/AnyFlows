import { Card } from '@heroui/react'
import { DashboardAvailabilityBody, type DashboardAvailabilityProps } from './dashboard-availability-body'

export function DashboardAvailability(props: DashboardAvailabilityProps) {
  return <Card className="border border-[var(--hairline)] bg-card" shadow="none"><DashboardAvailabilityBody {...props} /></Card>
}
