import type { EmailThread } from "../api/types";

export type ThreadFilter = string;

export function filterThreads(threads: EmailThread[], filter: ThreadFilter): EmailThread[] {
  if (filter === "all") return threads;
  if (filter === "answered") return threads.filter((thread) => thread.is_answered);
  if (filter === "unanswered") {
    return threads.filter((thread) => thread.is_valid_client_request && !thread.is_answered);
  }
  if (filter === "review") return threads.filter((thread) => thread.manual_review_required);
  return threads.filter((thread) => thread.classification === filter);
}
