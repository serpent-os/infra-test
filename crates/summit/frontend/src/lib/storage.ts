import { browser } from "$app/environment";

function storage<T>(key: string) {
  return {
    load(fallback: T) {
      if (browser) {
        let item = localStorage.getItem(key);
        if (item) {
          let parsed = JSON.parse(item);
          if (parsed) {
            return parsed as T;
          }
        }
      }
      return fallback;
    },
    update(value: T) {
      if (browser) {
        localStorage.setItem(key, JSON.stringify(value));
      }
    },
  };
}

export default storage;
