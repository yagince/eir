export type Reviewer = {
  login: string;
  avatar_url: string;
  state:
    | "approved"
    | "changes_requested"
    | "commented"
    | "dismissed"
    | "pending";
};

export type Commenter = {
  login: string;
  avatar_url: string;
};

export type CiStatus = "success" | "pending" | "failure" | "error" | "unknown";

export type WatchedItem = {
  id: number;
  kind: "pr" | "issue";
  title: string;
  number: number;
  repo: string;
  url: string;
  author: string;
  author_avatar: string;
  comments: number;
  updated_at: string;
  state: string;
  is_draft: boolean;
  reviewers: Reviewer[];
  commenters: Commenter[];
  ci_status: CiStatus | null;
};

export type NotificationItem = {
  thread_id: number;
  reason: string;
  repo: string;
  kind: "pr" | "issue" | "commit" | "discussion" | "release" | "other";
  number: number | null;
  title: string;
  url: string;
  updated_at: string;
};

export type Tab = "all" | "authored" | "review" | "mentions" | "hidden";

export type RepoGroup = {
  repo: string;
  items: WatchedItem[];
  mostRecent: string;
  unreadCount: number;
};
