export type OrganizeDecision = "keep" | "discard";

export type OrganizeLeftTab = "keep" | "discard";

export type OrganizeCenterLayout = "list" | "grid";

export type OrganizePreviewTab = "video" | "image" | "list";

export interface OrganizeItemRecord {
  itemId: string | null;
  path: string | null;
  name: string;
  kind: "File" | "Directory";
  decision: OrganizeDecision;
  updatedAt: string;
}

export interface OrganizeDirectoryState {
  version: 1;
  directoryPath: string;
  updatedAt: string;
  items: Record<string, OrganizeItemRecord>;
}
