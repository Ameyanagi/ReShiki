import { css } from "../../lib/color-reference";

export function GET() {
  return new Response(css(), {
    headers: { "Content-Type": "text/css; charset=utf-8" },
  });
}
