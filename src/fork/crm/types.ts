// Phase 9: what `fork/crm.rs` sends the UI. Field names are the camelCase of
// Rebound's DB columns (the Rust structs rename on serialise); every one is
// optional because the drawer must survive a column Rebound adds or drops.

export interface CrmWorkspace {
  id: string;
  name: string | null;
  slug: string | null;
  role: string | null;
}

export interface CrmStatus {
  configured: boolean;
  connected: boolean;
  workspaceId: string | null;
  /** Present when connected with no workspace chosen, or right after a sign-in. */
  workspaces: CrmWorkspace[] | null;
  baseUrl: string | null;
  supabaseUrl: string | null;
  /** `••••` plus the last four characters. */
  anonKeyMasked: string | null;
  email: string | null;
  /** Why the workspace list could not be fetched, when it was tried. */
  error: string | null;
}

export interface CrmPerson {
  id: string | null;
  workspaceId: string | null;
  firstName: string | null;
  lastName: string | null;
  fullName: string | null;
  primaryEmail: string | null;
  primaryPhone: string | null;
  linkedinUrl: string | null;
  twitterHandle: string | null;
  avatarUrl: string | null;
  bio: string | null;
  city: string | null;
  state: string | null;
  country: string | null;
  timezone: string | null;
  tags: string[] | null;
  ownerUserId: string | null;
  customFields: Record<string, unknown> | null;
  externalIds: Record<string, unknown> | null;
  mergedIntoId: string | null;
  lastActivityAt: string | null;
  createdAt: string | null;
  updatedAt: string | null;
}

export interface CrmCompany {
  id: string | null;
  workspaceId: string | null;
  name: string | null;
  domain: string | null;
  website: string | null;
  industry: string | null;
  description: string | null;
  employeeCountRange: string | null;
  linkedinUrl: string | null;
  logoUrl: string | null;
  addressLine1: string | null;
  addressLine2: string | null;
  city: string | null;
  state: string | null;
  postalCode: string | null;
  country: string | null;
  tags: string[] | null;
  ownerUserId: string | null;
  customFields: Record<string, unknown> | null;
  externalIds: Record<string, unknown> | null;
  mergedIntoId: string | null;
  lastActivityAt: string | null;
  createdAt: string | null;
  updatedAt: string | null;
}

export interface CrmDeal {
  id: string | null;
  name: string | null;
  value: number | null;
  /** Rebound `deal_status`: open | won | lost | ... */
  status: string | null;
  stageId: string | null;
  /** Resolved from `pipeline_stages` by Rust, best effort. */
  stageName: string | null;
}

export interface CrmActivity {
  id: string | null;
  workspaceId: string | null;
  entityType: string | null;
  entityId: string | null;
  /** Rebound `activity_type`, e.g. `email_received`, `deal_stage_changed`. */
  activityType: string | null;
  actorType: string | null;
  actorUserId: string | null;
  occurredAt: string | null;
  createdAt: string | null;
  payload: Record<string, unknown> | null;
  relatedEntities: unknown;
  externalSource: string | null;
  externalId: string | null;
  pendingReplyId: string | null;
}

export interface CrmReminder {
  id: string | null;
  title: string | null;
  description: string | null;
  remindAt: string | null;
  snoozedUntil: string | null;
  status: string | null;
  entityType: string | null;
  entityId: string | null;
}

/** Mirrors `POST /api/v1/extension/lookup`: `{person, company, deals[], activities[]}`. */
export interface CrmLookup {
  person: CrmPerson | null;
  company: CrmCompany | null;
  deals: CrmDeal[];
  activities: CrmActivity[];
  /** Not sent by the route today; rendered when it is. */
  reminders: CrmReminder[] | null;
}

/** `SkimError` on the wire. */
export interface CrmError {
  code: string;
  message: string;
}

export const CRM_SETUP_CODES = new Set(["crm_not_configured", "crm_not_connected", "crm_no_workspace"]);
