const settingsFocusClasses =
  "focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-focus";

export const settingsControlClasses = `max-w-full min-h-(--control-height) rounded-control border border-control-edge bg-field px-2 py-[0.2rem] text-[0.875rem] text-inherit disabled:text-muted ${settingsFocusClasses}`;

export const settingsButtonClasses = `${settingsControlClasses} enabled:[&:hover]:bg-hover`;

export const settingsInputClasses = `accent-focus disabled:text-muted ${settingsFocusClasses}`;

export const settingsChoiceClasses =
  "flex flex-wrap items-center justify-between gap-2 text-[0.875rem]";
