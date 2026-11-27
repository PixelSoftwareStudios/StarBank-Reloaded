import * as Tooltip from "@radix-ui/react-tooltip";
import type { ComponentProps, ReactElement } from "react";

import { tooltipClasses } from "./control-styles";

const EDITOR_TOOLTIP_DELAY_MS = 400;

interface TooltipOptions {
  help: string;
  placement?: "top" | "bottom";
  disabled?: boolean;
}

export function EditorTooltip({
  help,
  placement = "top",
  disabled,
  children,
}: TooltipOptions & { children: ReactElement }) {
  if (disabled) return children;
  return (
    <Tooltip.Provider
      delayDuration={EDITOR_TOOLTIP_DELAY_MS}
      disableHoverableContent
    >
      <Tooltip.Root>
        <Tooltip.Trigger asChild>{children}</Tooltip.Trigger>
        <Tooltip.Portal>
          <Tooltip.Content
            className={tooltipClasses}
            side={placement}
            sideOffset={5}
          >
            {help}
          </Tooltip.Content>
        </Tooltip.Portal>
      </Tooltip.Root>
    </Tooltip.Provider>
  );
}

export function EditorHelpButton({
  help,
  placement,
  disabled,
  ...props
}: TooltipOptions & ComponentProps<"button">) {
  return (
    <EditorTooltip help={help} placement={placement} disabled={disabled}>
      <button type="button" {...props} disabled={disabled} />
    </EditorTooltip>
  );
}
