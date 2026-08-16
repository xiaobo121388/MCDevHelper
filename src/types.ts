export type ComponentKind = "addon" | "material" | "map";
export type SourceKind = "mcs_auto" | "single" | "library";
export type IdentityPolicy = "preserve" | "regenerate" | "error";
export type VersionPart = "major" | "minor" | "patch";
export type ContentMode = "clean" | "full";
export type ThemePreference = "light" | "dark" | "system";

export interface AppSettings {
  developer_nickname: string;
  developer_account: string;
  developer_user_id: string;
  default_destination?: string;
  theme: ThemePreference;
}

export interface SourceRecord {
  id: string;
  kind: SourceKind;
  path: string;
}

export interface ManifestSummary {
  path: string;
  name?: string;
  header_uuid?: string;
  version?: [number, number, number];
  module_types: string[];
}

export interface ComponentSummary {
  id: string;
  name: string;
  kind: ComponentKind;
  path: string;
  origin: { kind: "mcs" | "single" | "library"; source_path?: string; source_id?: string };
  mcs?: { uid: string; component_type: number; account?: string; category: string };
  manifests: ManifestSummary[];
  version?: [number, number, number];
  tags: string[];
  favorite: boolean;
  icon_path?: string;
  updated_at?: string;
  modified_at?: string;
  created_at?: string;
  size_bytes: number;
}

export interface DiscoveryWarning {
  path: string;
  message: string;
}

export interface DiscoveryResult {
  components: ComponentSummary[];
  sources: SourceRecord[];
  warnings: DiscoveryWarning[];
}

export interface OperationResult {
  component?: ComponentSummary;
  actual_path: string;
  modified_files: string[];
  warnings: string[];
}

export interface UpdateCheckResult {
  current_version: string;
  latest_version?: string;
  release_name?: string;
  release_url?: string;
  published_at?: string;
  update_available: boolean;
  no_release: boolean;
}

export interface CoreError {
  code: string;
  message: string;
  path?: string;
  details?: unknown;
}
