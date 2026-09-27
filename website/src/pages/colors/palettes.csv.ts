import { csv } from "../../lib/color-reference";

export function GET() {
  return new Response(csv(), {
    headers: { "Content-Type": "text/csv; charset=utf-8" },
  });
}
