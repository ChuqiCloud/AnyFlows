import { useEffect, useState } from "react";
import { Icon } from "@iconify/react";
import { cn, Skeleton } from "@heroui/react";

interface SidebarIconProps {
  icon: string;
  width?: number;
  className?: string;
}

export const SidebarIcon = ({
  icon,
  width = 20,
  className,
}: SidebarIconProps) => {
  const [isLoaded, setIsLoaded] = useState(false);
  const [hasError, setHasError] = useState(false);

  useEffect(() => {
    setIsLoaded(false);
    setHasError(false);
  }, [icon]);

  return (
    <div
      className="relative flex items-center justify-center"
      style={{ width, height: width }}
    >
      {!isLoaded && (
        <Skeleton
          className={cn("absolute inset-0 rounded-md", "animate-pulse")}
        />
      )}

      {hasError && (
        <div
          className={cn(
            "flex items-center justify-center rounded-md bg-default-100",
            className,
          )}
          style={{ width, height: width }}
        >
          <span className="text-default-300 text-xs">?</span>
        </div>
      )}

      {!hasError && (
        <Icon
          className={cn(
            "transition-opacity duration-200",
            isLoaded ? "opacity-100" : "opacity-0",
            className,
          )}
          icon={icon}
          width={width}
          onError={() => {
            setHasError(true);
            setIsLoaded(true);
          }}
          onLoad={() => setIsLoaded(true)}
        />
      )}
    </div>
  );
};
