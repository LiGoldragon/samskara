@0xc3b2e1d5a7f6c9d1;

interface SamskaraWorld {
  query @0 (script :Text) -> (result :Text);
  # Executes a CozoScript query and returns the result as a JSON string.

  put @1 (script :Text) -> ();
  # Executes a CozoScript mutation (e.g., :put, :rm).

  rescan @2 () -> ();
  # Triggers a filesystem scan to update the World with latest component/file metadata.

  getStatus @3 () -> (status :Status);
  # Returns the current status of the World daemon.

  struct Status {
    version @0 :Text;
    dbPath @1 :Text;
    componentCount @2 :UInt32;
    fileCount @3 :UInt32;
  }
}
