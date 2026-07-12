/// <reference types="vite/client" />

interface ImportMetaEnv {
  readonly VITE_TRAWLER_API?: string;
  readonly VITE_E2E?: string;
}

interface ImportMeta {
  readonly env: ImportMetaEnv;
}
