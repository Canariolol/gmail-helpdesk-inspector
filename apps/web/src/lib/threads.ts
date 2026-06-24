import type { EmailThread } from "../api/types";

export type ThreadFilter = string;

export function filterThreads(threads: EmailThread[], filter: ThreadFilter): EmailThread[] {
  if (filter === "all") return threads;
  if (filter === "answered") {
    return threads.filter(
      (thread) =>
        thread.classification === "valid_client_request" && thread.is_answered,
    );
  }
  if (filter === "unanswered") {
    return threads.filter(
      (thread) =>
        thread.classification === "valid_client_request" && !thread.is_answered,
    );
  }
  if (filter === "review") {
    return threads.filter((thread) => thread.manual_review_required);
  }
  if (filter === "ignored") {
    return threads.filter(
      (thread) =>
        thread.classification !== "valid_client_request"
        && thread.classification !== "ambiguous",
    );
  }
  return threads.filter((thread) => thread.classification === filter);
}
