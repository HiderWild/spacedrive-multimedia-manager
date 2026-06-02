import { useExplorerKeyboard } from "./hooks/useExplorerKeyboard";
import { useKeyboardNavigation } from "./hooks/useKeyboardNavigation";

/**
 * Invisible component that handles keyboard events
 * Rendered separately to avoid causing parent rerenders
 */
export function KeyboardHandler() {
  useExplorerKeyboard();
  useKeyboardNavigation();
  return null;
}