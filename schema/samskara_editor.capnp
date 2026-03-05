@0xe4b2c1d3a5f6b7c8;

# This schema defines the programmatic editor redirection for Saṃskāra.
# It captures the intent of a VCS operation and allows for structured refinement.

struct EditorRequest {
  filePath @0 :Text;
  originalContent @1 :Text;
  operationType @2 :OperationType;
  
  enum OperationType {
    commit @0;
    squash @1;
    rebaseInteractive @2;
    description @3;
  }

  context @3 :Text; # Rich metadata from the active intent
  requestingAgentId @4 :Text;
}

struct EditorResponse {
  refinedContent @0 :Text;
  status @1 :Status;
  
  enum Status {
    success @0;
    aborted @1;
    needsHumanReview @2;
    triggerSubflow @3;
  }

  # Structured UI Intent for cross-platform rendering (Pi, vtcode, Flutter)
  uiIntent @2 :UiIntent;

  struct UiIntent {
    componentId @0 :Text; # e.g., "mentci.vcs.intent-diff"
    data @1 :Data;        # Cap'n Proto encoded data for the UI component
  }

  subflowTarget @3 :Text; # The specific tool/contract to trigger if status is triggerSubflow
}
