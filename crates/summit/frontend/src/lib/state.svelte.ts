import type { Endpoint } from "./api";
import * as api from "./api";
import storage from "./storage";

const refreshStorage = storage<number>("refreshSeconds");

export const useState = () => {
  let refreshSeconds = $state(refreshStorage.load(10));
  let endpoints: Endpoint[] = $state([]);

  const fetchEndpoints = async () => {
    let resp = await api.endpoints();
    if ("data" in resp) {
      endpoints = resp.data.endpoints;
    }
  };

  // Only runs once - endpoints $state isn't captured directly
  // so fetchEndpoints doesn't cause this effect to reload.
  $effect(() => {
    fetchEndpoints();
  });

  // Reruns everytime refreshSeconds $state is changed, updating
  // storage & interval
  $effect(() => {
    refreshStorage.update(refreshSeconds);

    let refreshEndpoints = setInterval(fetchEndpoints, refreshSeconds * 1000);

    return () => {
      clearInterval(refreshEndpoints);
    };
  });

  return {
    get endpoints() {
      return endpoints;
    },
    get refreshSeconds() {
      return refreshSeconds;
    },
    set refreshSeconds(seconds: number) {
      refreshSeconds = Math.max(seconds, 1);
    },
  };
};
