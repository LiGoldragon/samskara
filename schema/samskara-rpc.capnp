@0xb78dc5a43a1e54d1;

interface Samskara {
  query @0 (script :Data) -> (result :Data);
  # Execute CozoScript against the world database.
  # script: CozoScript text as bytes
  # result: CozoScript tuple output as bytes

  listRelations @1 () -> (result :Data);
  # List all stored relations.

  describeRelation @2 (name :Data) -> (result :Data);
  # Show schema of a specific relation.

  commitWorld @3 (message :Data, agentId :Data) -> (commitHash :Data);
  # Commit staged changes to the world.

  restoreWorld @4 (commitId :Data) -> (result :Data);
  # Restore world state from a commit.

  assertThought @5 (kind :Data, scope :Data, status :Data, title :Data, body :Data) -> (titleHash :Data);
  # Assert a new thought into the world model.

  queryThoughts @6 (kind :Data, scope :Data, tag :Data, phase :Data) -> (result :Data);
  # Query thoughts with optional filters.
}
