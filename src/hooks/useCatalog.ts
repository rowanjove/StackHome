import { useCallback, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import type {
  CatalogQuery,
  CatalogScanRequest,
  CatalogScanResult,
  FileRecord,
} from "../types/backup";

export function useCatalog() {
  const [files, setFiles] = useState<FileRecord[]>([]);
  const [scanResult, setScanResult] = useState<CatalogScanResult | null>(null);
  const [scanning, setScanning] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const query = useCallback(async (request: CatalogQuery) => {
    try {
      const result = await invoke<FileRecord[]>("catalog_query", { request });
      setFiles(result);
      return result;
    } catch (reason) {
      setError(String(reason));
      return [];
    }
  }, []);

  const scan = useCallback(
    async (request: CatalogScanRequest) => {
      setScanning(true);
      setError(null);
      try {
        const result = await invoke<CatalogScanResult>("catalog_scan", { request });
        setScanResult(result);
        await query({
          search: "",
          rootPath: request.rootPath,
          limit: 10_000,
          offset: 0,
        });
        return result;
      } catch (reason) {
        setError(String(reason));
        return null;
      } finally {
        setScanning(false);
      }
    },
    [query],
  );

  return { files, scanResult, scanning, error, setError, query, scan };
}
