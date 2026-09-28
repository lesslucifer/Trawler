import { UNREACHABLE, type TrawlerApi } from "./api";

// Test support: a whole TrawlerApi whose every method answers emptily, so a test
// overrides only the calls it is about. Without it each suite spells out the
// full surface and every method added to the client edits five unrelated files.
export function apiStub(over: Partial<TrawlerApi> = {}): TrawlerApi {
  return {
    listFlows: async () => [],
    latestCapture: async () => null,
    exportHar: async () => ({ bytes: new Uint8Array(), filename: "trawler.har" }),
    importHar: async () => 0,
    importCurl: async () => 0,
    clearFlows: async () => 0,
    caCertPem: async () => "",
    caFile: async () => new Uint8Array(),
    getFlow: async () => {
      throw new Error("getFlow is not stubbed for this test");
    },
    getBody: async () => ({ bytes: new Uint8Array(), contentType: null }),
    reach: async () => UNREACHABLE,
    armReach: async () => UNREACHABLE,
    disarmReach: async () => UNREACHABLE,
    replay: async () => {
      throw new Error("replay is not stubbed for this test");
    },
    send: async () => {
      throw new Error("send is not stubbed for this test");
    },
    stream: () => () => {},
    ...over,
  };
}
