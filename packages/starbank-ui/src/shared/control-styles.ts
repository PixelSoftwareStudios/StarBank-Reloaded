export {
  settingsButtonClasses,
  settingsChoiceClasses,
  settingsControlClasses,
  settingsInputClasses,
} from "../settings/settings-controls";

export const focusOutlineClasses =
  "focus-visible:[outline:2px_solid_var(--map-focus)] focus-visible:-outline-offset-2";

const buttonBase = `editor-button min-h-[var(--control-height)] rounded-control border px-[0.6rem] py-[0.2rem] disabled:cursor-not-allowed disabled:opacity-55 data-disabled:cursor-not-allowed data-disabled:opacity-55 ${focusOutlineClasses}`;

const buttonVariants = {
  default:
    "border-control-edge bg-field text-ink [&:hover:not(:disabled)]:border-focus [&:hover:not(:disabled)]:bg-hover",
  primary:
    "editor-primary border-transparent bg-[var(--editor-primary-bg)] font-semibold text-[color:var(--editor-primary-text)] [&:hover:not(:disabled)]:bg-[color-mix(in_srgb,var(--editor-primary-bg),white_11%)]",
  reset: "editor-reset border-transparent bg-transparent text-ink",
};

export function buttonClasses(
  variant: keyof typeof buttonVariants = "default",
) {
  return `${buttonBase} ${buttonVariants[variant]}`;
}

const popupClasses =
  "z-20 rounded-control border border-line bg-surface text-ink shadow-[0_2px_10px_rgb(0_0_0/18%)]";

export const menuContentClasses = `editor-action-popover ${popupClasses} min-w-40 max-w-[calc(100vw_-_1rem)] max-h-[var(--radix-dropdown-menu-content-available-height)] overflow-auto p-1`;
export const menuItemClasses =
  "editor-menu-item cursor-default rounded-sm px-2 py-1.5 text-[0.875rem] [outline:none] data-disabled:text-muted data-disabled:opacity-70 data-highlighted:bg-hover";
export const menuDividerClasses = "editor-menu-divider my-1 h-px bg-line";
export const tooltipClasses = `editor-tooltip ${popupClasses} max-w-72 px-2 py-[0.3rem] text-[0.8125rem] wrap-anywhere`;

export const dialogClasses =
  "editor-dialog m-auto max-h-[calc(100vh_-_1.5rem)] w-[min(29rem,calc(100vw_-_1.5rem))] overflow-auto rounded-lg border border-line bg-canvas p-3 text-ink backdrop:bg-black/45";
export const dialogHeaderClasses =
  "editor-dialog-header mb-[0.6rem] flex items-center justify-between gap-2";
