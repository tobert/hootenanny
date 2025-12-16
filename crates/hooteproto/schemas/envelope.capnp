@0xd9c43dd35faffe92;

using Common = import "common.capnp";
using Tools = import "tools.capnp";
using Garden = import "garden.capnp";

# Message envelope for all ZMQ communication
struct Envelope {
  id @0 :Common.Uuid;
  traceparent @1 :Text;
  payload @2 :Payload;
}

# All message types in the system
struct Payload {
  union {
    # === Worker Management ===
    register @0 :Common.WorkerRegistration;
    ping @1 :Void;
    pong @2 :Pong;
    shutdown @3 :ShutdownRequest;

    # === Tool Requests ===
    toolRequest @4 :Tools.ToolRequest;

    # === Garden/Timeline (direct, not through tools) ===
    gardenStatus @5 :Void;
    gardenPlay @6 :Void;
    gardenPause @7 :Void;
    gardenStop @8 :Void;
    gardenSeek @9 :Garden.Seek;
    gardenSetTempo @10 :Garden.SetTempo;
    gardenQuery @11 :Garden.Query;
    gardenEmergencyPause @12 :Void;
    gardenCreateRegion @13 :Garden.CreateRegion;
    gardenDeleteRegion @14 :Garden.DeleteRegion;
    gardenMoveRegion @15 :Garden.MoveRegion;
    gardenGetRegions @16 :Garden.GetRegions;

    # === Transport (direct to chaosgarden) ===
    transportPlay @17 :Void;
    transportStop @18 :Void;
    transportSeek @19 :Garden.TransportSeek;
    transportStatus @20 :Void;

    # === Timeline (direct to chaosgarden) ===
    timelineQuery @21 :Garden.TimelineQuery;
    timelineAddMarker @22 :Garden.TimelineAddMarker;
    timelineEvent @23 :Garden.TimelineEvent;

    # === Responses ===
    success @24 :Success;
    error @25 :Common.Error;
    toolList @26 :ToolList;
  }
}

struct Pong {
  workerId @0 :Common.Uuid;
  uptimeSecs @1 :UInt64;
}

struct ShutdownRequest {
  reason @0 :Text;
}

struct Success {
  result @0 :Text;  # JSON string
}

struct ToolList {
  tools @0 :List(Common.ToolInfo);
}
