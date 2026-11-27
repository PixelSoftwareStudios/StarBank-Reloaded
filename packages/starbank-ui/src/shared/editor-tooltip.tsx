import { Tooltip } from "@heroui/react/tooltip";
import type { ComponentProps, ReactNode } from "react";

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
}: TooltipOptions & { children: ReactNode }) {
  return (
    <Tooltip
      delay={EDITOR_TOOLTIP_DELAY_MS}
      closeDelay={0}
      isDisabled={disabled}
    >
      {children}
      <Tooltip.Content className="editor-tooltip" placement={placement}>
        {help}
      </Tooltip.Content>
    </Tooltip>
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
      <Tooltip.Trigger<"button">
        {...props}
        disabled={disabled}
        render={(trigger) => <button type="button" {...trigger} />}
      />
    </EditorTooltip>
  );
}
