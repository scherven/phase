/**
 * Minimal fake matching the subset of PeerJS's `DataConnection` API surface
 * that `createPeerSession` and the P2P adapter integration tests use.
 *
 * Shared between protocol unit tests (`peer.test.ts`) and adapter integration
 * tests (`p2p-adapter-multiplayer.test.ts`).
 */

type DataHandler = (data: unknown) => void;
type VoidHandler = () => void;
type ErrorHandler = (err: Error) => void;

export class FakeDataConnection {
  open = true;
  sent: unknown[] = [];

  private dataHandlers = new Set<DataHandler>();
  private closeHandlers = new Set<VoidHandler>();
  private errorHandlers = new Set<ErrorHandler>();

  send(data: unknown) {
    if (!this.open) throw new Error("Connection is closed");
    this.sent.push(data);
  }

  close() {
    if (this.open) this.simulateClose();
  }

  on(event: string, handler: (...args: unknown[]) => void): this {
    if (event === "data") this.dataHandlers.add(handler as DataHandler);
    else if (event === "close") this.closeHandlers.add(handler as VoidHandler);
    else if (event === "error") this.errorHandlers.add(handler as ErrorHandler);
    return this;
  }

  off(event: string, handler: (...args: unknown[]) => void): this {
    if (event === "data") this.dataHandlers.delete(handler as DataHandler);
    else if (event === "close") this.closeHandlers.delete(handler as VoidHandler);
    else if (event === "error") this.errorHandlers.delete(handler as ErrorHandler);
    return this;
  }

  // ── Test helpers ──
  simulateData(data: unknown) {
    for (const h of this.dataHandlers) h(data);
  }

  simulateClose() {
    this.open = false;
    for (const h of this.closeHandlers) h();
  }

  simulateError(err: Error) {
    for (const h of this.errorHandlers) h(err);
  }
}
