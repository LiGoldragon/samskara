@0x8a9b7c6d5e4f3a2b;

struct Sandbox {
  id @0 :Text;
  path @1 :Text;
  agentId @2 :Text;
  bookmark @3 :Text;
  status @4 :Status;
  intent @5 :Text;

  enum Status {
    active @0;
    completed @1;
    failed @2;
    paused @3;
  }
}

interface SandboxManager {
  registerSandbox @0 (sandbox :Sandbox) -> ();
  updateStatus @1 (id :Text, status :Sandbox.Status) -> ();
  getSandbox @2 (id :Text) -> (sandbox :Sandbox);
  listSandboxes @3 () -> (sandboxes :List(Sandbox));
  logEvent @4 (id :Text, eventType :Text, message :Text) -> ();
}
