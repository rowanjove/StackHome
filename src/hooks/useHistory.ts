import { useCallback, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { OperationHistoryItem } from "../types/backup";

export function useHistory(limit = 200) {
  const [entries, setEntries] = useState<OperationHistoryItem[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  const refresh = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      setEntries(await invoke<OperationHistoryItem[]>("history_list", { limit }));
    } catch (reason) {
      setError(String(reason));
    } finally {
      setLoading(false);
    }
  }, [limit]);

  useEffect(() => { void refresh(); }, [refresh]);

  return { entries, loading, error, refresh, setEntries };
}
