// The tasks app's texts, in the six languages, through the core's kit.
import {
  appText,
  registerAppMessages,
  type LocalizeController,
  type Message,
} from "ortus-kit";
import en, { type TasksKey } from "./locales/en.js";

const catalogs: Record<
  string,
  () => Promise<{ default: Record<string, Message> }>
> = {
  pl: () => import("./locales/pl.js"),
  uk: () => import("./locales/uk.js"),
  de: () => import("./locales/de.js"),
  fr: () => import("./locales/fr.js"),
  ar: () => import("./locales/ar.js"),
};

export const ready = registerAppMessages("tasks", en, async (code) =>
  code === "en" ? en : ((await catalogs[code]?.())?.default ?? {}),
);

export function t(
  localize: LocalizeController,
  key: TasksKey,
  args?: Record<string, string | number>,
): string {
  return appText(localize, "tasks", key, args);
}
