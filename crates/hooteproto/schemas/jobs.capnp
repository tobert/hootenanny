@0xe73698fcef0e99f0;

using Common = import "common.capnp";

struct JobInfo {
  jobId @0 :Text;
  status @1 :Common.JobStatus;
  source @2 :Text;
  result @3 :Text;  # JSON string
  error @4 :Text;
  createdAt @5 :UInt64;
  startedAt @6 :UInt64;
  completedAt @7 :UInt64;
}

struct JobStoreStats {
  total @0 :UInt32;
  pending @1 :UInt32;
  running @2 :UInt32;
  completed @3 :UInt32;
  failed @4 :UInt32;
  cancelled @5 :UInt32;
}

# Job-related requests
struct JobExecute {
  scriptHash @0 :Text;
  params @1 :Text;  # JSON
  tags @2 :List(Text);
}

struct JobStatusRequest {
  jobId @0 :Text;
}

struct JobPoll {
  jobIds @0 :List(Text);
  timeoutMs @1 :UInt64;
  mode @2 :Common.PollMode;
}

struct JobCancel {
  jobId @0 :Text;
}

struct JobList {
  status @0 :Text;
}
