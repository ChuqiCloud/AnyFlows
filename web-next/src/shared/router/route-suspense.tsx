import {
  Suspense,
  type ReactNode,
} from "react";

import { PageLoading } from "@/shared/components/loading";

export const withRouteSuspense = (
  element: ReactNode,
  fallback: ReactNode = <PageLoading />,
) => {
  return (
    <Suspense fallback={fallback}>
      {element}
    </Suspense>
  );
};
