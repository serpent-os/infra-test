const BASE = `/api/v1`;

async function get<T>(path: string): Promise<Body<T>> {
  let resp = await fetch(`${BASE}/${path}`, {
    headers: {
      "content-type": "application/json",
    },
  });
  return await resp.json();
}

export async function endpoints(): Promise<Body<{ endpoints: Endpoint[] }>> {
  return await get("endpoints");
}

export type Body<T> =
  | { success: boolean; data: T }
  | { success: boolean; error: string };

export interface Endpoint {
  id: string;
  host_address: string;
  status: string;
  error?: string;
}

export default {
  endpoints,
};
