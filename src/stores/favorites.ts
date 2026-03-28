import { createSignal } from "solid-js";

// Global version counter — incremented whenever any favorite is toggled.
// SessionView watches this to re-check its starred state.
const [favoriteVersion, setFavoriteVersion] = createSignal(0);

export function bumpFavoriteVersion() {
  setFavoriteVersion((v) => v + 1);
}

export { favoriteVersion };
