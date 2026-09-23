// Fork (Phase 9): fixtures for the Rebound drawer in the demo harness. The
// main session wires them into `tauri-core.ts`:
//   case "fork_crm_status": return ok(forkCrmStatus());
//   case "fork_crm_lookup": return forkCrmLookup(String(args.email ?? ""));
//   case "fork_crm_connect": case "fork_crm_pick_workspace":
//   case "fork_crm_disconnect": case "fork_crm_set_config": return ok(forkCrmStatus());
//
// Switch: localStorage `skimdemo.fork_crm` = "off" renders the not-connected
// state; anything else is connected. One fixture address is found (the hero
// thread's sender, Anna Weber); every other address is "Not in Rebound".
import type { CrmLookup, CrmStatus } from "../../src/fork/crm/types";

const FOUND_EMAIL = "anna.weber@northwind.example";
const DAY = 86_400_000;
const NOW = Date.now();
const iso = (msAgo: number) => new Date(NOW - msAgo).toISOString();

function connected(): boolean {
  try {
    return (globalThis as any).localStorage?.getItem("skimdemo.fork_crm") !== "off";
  } catch {
    return true;
  }
}

export function forkCrmStatus(): CrmStatus {
  const on = connected();
  return {
    configured: true,
    connected: on,
    workspaceId: on ? "ws-demo" : null,
    workspaces: on ? [{ id: "ws-demo", name: "Brightgro", slug: "brightgro", role: "owner" }] : null,
    baseUrl: "https://rebound.patricknesbitt.ai",
    supabaseUrl: "https://demo.supabase.co",
    anonKeyMasked: "••••k9Qw",
    email: on ? "patrick@autospark.ai" : null,
    error: null,
  };
}

export const FORK_CRM_FOUND: CrmLookup = {
  person: {
    id: "8f3c1b2a-0000-4000-8000-000000000001",
    workspaceId: "ws-demo",
    firstName: "Anna",
    lastName: "Weber",
    fullName: "Anna Weber",
    primaryEmail: FOUND_EMAIL,
    primaryPhone: "+49 30 1234 5678",
    linkedinUrl: "https://www.linkedin.com/in/anna-weber-demo",
    twitterHandle: null,
    avatarUrl: null,
    bio: null,
    city: "Berlin",
    state: null,
    country: "Germany",
    timezone: "Europe/Berlin",
    tags: ["ops", "q3-launch"],
    ownerUserId: "u-demo",
    customFields: { seniority: "VP" },
    externalIds: null,
    mergedIntoId: null,
    lastActivityAt: iso(2 * 3_600_000),
    createdAt: iso(120 * DAY),
    updatedAt: iso(2 * 3_600_000),
  },
  company: {
    id: "8f3c1b2a-0000-4000-8000-0000000000c1",
    workspaceId: "ws-demo",
    name: "Northwind Logistics",
    domain: "northwind.example",
    website: "https://northwind.example",
    industry: "Logistics",
    description: null,
    employeeCountRange: "51-200",
    linkedinUrl: null,
    logoUrl: null,
    addressLine1: null,
    addressLine2: null,
    city: "Berlin",
    state: null,
    postalCode: null,
    country: "Germany",
    tags: null,
    ownerUserId: null,
    customFields: null,
    externalIds: null,
    mergedIntoId: null,
    lastActivityAt: null,
    createdAt: iso(120 * DAY),
    updatedAt: null,
  },
  deals: [
    { id: "d-1", name: "Q3 rollout: AI order desk", value: 45_000, status: "open", stageId: "s-2", stageName: "Proposal" },
    { id: "d-2", name: "Warehouse pilot", value: 12_500, status: "open", stageId: "s-1", stageName: "Discovery" },
    { id: "d-3", name: "2025 renewal", value: 9_000, status: "won", stageId: "s-9", stageName: "Won" },
  ],
  activities: [
    { id: "a-1", activityType: "email_received", occurredAt: iso(2 * 3_600_000), payload: { subject: "Q3 launch — final checklist & open questions" } },
    { id: "a-2", activityType: "email_sent", occurredAt: iso(1 * DAY), payload: { subject: "Re: pricing for the order desk" } },
    { id: "a-3", activityType: "deal_stage_changed", occurredAt: iso(3 * DAY), payload: { to_stage: "Proposal" } },
    { id: "a-4", activityType: "meeting_completed", occurredAt: iso(6 * DAY), payload: { title: "Discovery call" } },
    { id: "a-5", activityType: "note_created", occurredAt: iso(9 * DAY), payload: { note: "Wants a 30-day pilot before committing." } },
    { id: "a-6", activityType: "person_created", occurredAt: iso(120 * DAY), payload: {} },
  ].map((a) => ({
    id: a.id,
    workspaceId: "ws-demo",
    entityType: "person",
    entityId: "8f3c1b2a-0000-4000-8000-000000000001",
    activityType: a.activityType,
    actorType: "user",
    actorUserId: "u-demo",
    occurredAt: a.occurredAt,
    createdAt: a.occurredAt,
    payload: a.payload,
    relatedEntities: null,
    externalSource: null,
    externalId: null,
    pendingReplyId: null,
  })),
  reminders: [
    { id: "r-1", title: "Send the pilot SOW", description: null, remindAt: iso(-2 * DAY), snoozedUntil: null, status: "pending", entityType: "person", entityId: "8f3c1b2a-0000-4000-8000-000000000001" },
  ],
};

export const FORK_CRM_EMPTY: CrmLookup = { person: null, company: null, deals: [], activities: [], reminders: null };

/** Rejects like Rust does when the drawer is in the not-connected state. */
export function forkCrmLookup(email: string): Promise<CrmLookup> {
  if (!connected()) return Promise.reject({ code: "crm_not_connected", message: "Not signed in to Rebound" });
  return Promise.resolve(email.trim().toLowerCase() === FOUND_EMAIL ? FORK_CRM_FOUND : FORK_CRM_EMPTY);
}
