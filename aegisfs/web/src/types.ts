export type BlockFeatures = {
  offset: number;
  bytes: number;
  entropy_pre: number;
  entropy_post: number;
  delta_h: number;
  chi2: number | null;
  chi2_p: number | null;
  kl_post_pre_bits: number;
  serial_correlation_post: number | null;
  header_before: string | null;
  header_after: string | null;
  header_destroyed: boolean;
};

export type ProcessView = {
  ragid: string;
  root_pid: number;
  worker_pids: number[];
  executable: string;
  status: string;
  writes: number;
  delta_h: number;
  score: number;
  peak_score: number;
  started_ns: number;
  containment_ms: number | null;
  trajectory: { elapsed_ms: number; score: number }[];
};

export type Observation = {
  sequence: number;
  timestamp_ns: number;
  ragid: string;
  path: string;
  operation: string;
  canary: boolean;
  crypto_uniform: boolean;
  features: BlockFeatures;
};

export type TxEntry = {
  id: number;
  ragid: string;
  timestamp_ns: number;
  operation: string;
  path: string;
  new_path: string | null;
  pre_hash: string | null;
  post_hash: string | null;
  pre_bytes: number;
  state: string;
};

export type RunView = {
  id: string;
  ragid: string;
  workload: string;
  status: string;
  touched_files: number;
  modified_bytes: number;
  contained: boolean;
  recovered: boolean;
  restored_operations: number;
  restored_bytes: number;
  pre_tree_blake3: string | null;
  post_tree_blake3: string | null;
  error: string | null;
};

export type Canary = {
  path: string;
  format: string;
  blake3: string;
  hidden: boolean;
  bytes: number;
};

export type Telemetry = {
  status: {
    schema_version: number;
    mode: string;
    protection_active: boolean;
    mount: string;
    transport: string;
    active_run: string | null;
    cow_bytes: number;
    threshold: number;
    warning: string;
  };
  processes: ProcessView[];
  observations: Observation[];
  ledger: TxEntry[];
  runs: RunView[];
  canaries: Canary[];
  alerts: { timestamp_ns: number; severity: string; ragid: string; message: string }[];
};
