// DEC-56: one request, on the user's click, to a destination the sentence
// names and Trawler does not own. The window's own fetch goes the way a browser
// on this machine goes — through its system proxy, checked against its trust
// store — so what Trawler captures of it is the reading. Its own outcome says
// only whether it completed: an opaque answer is all a cross-origin request may
// read, and all the button needs.
export const REQUEST_HOST = "example.com";

export async function makeRequest(): Promise<void> {
  await fetch(`https://${REQUEST_HOST}/`, { mode: "no-cors", cache: "no-store" });
}
