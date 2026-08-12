import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";
import rehypeHighlight from "rehype-highlight";

/// Render a proof document.
///
/// Kept in step with the copy in the vulnhash server repo. The two are
/// separate projects — one open source, one not — so this is duplicated
/// deliberately rather than shared through a package.
///
/// Published proofs are written by strangers and shown to other strangers —
/// the vendor being disputed, other researchers — so this is the one place in
/// the site where attacker-supplied content becomes a page. Three decisions
/// follow from that, and none of them should be quietly relaxed later:
///
/// Raw HTML is not rendered. react-markdown ignores embedded HTML unless
/// rehype-raw is added, so a <script> or an onerror attribute in a proof
/// document comes out as text. That is a property of the pipeline rather than
/// of a sanitiser being called correctly at each site, which is the failure
/// mode this project has already had once with attachments.
///
/// Links are limited to protocols react-markdown considers safe, which rules
/// out javascript: URLs. External ones open in a new tab with rel=noreferrer,
/// so a proof document cannot learn where its reader came from.
///
/// Images are not loaded. A remote image in a published finding would report
/// the IP address of everyone who reads it back to whoever hosts it — the
/// vendor reading a disclosure, other researchers, anyone. Since a proof is
/// meant to be evidence rather than a page, images render as a link the reader
/// can choose to follow.
export default function Markdown({ children }: { children: string }) {
  return (
    <div className="markdown">
      <ReactMarkdown
        remarkPlugins={[remarkGfm]}
        rehypePlugins={[rehypeHighlight]}
        components={{
          a: ({ href, children }) => (
            <a href={href} target="_blank" rel="noopener noreferrer nofollow">
              {children}
            </a>
          ),
          img: ({ src, alt }) => (
            <a className="markdown__image" href={typeof src === "string" ? src : undefined}
               target="_blank" rel="noopener noreferrer nofollow">
              {alt || "image"} ({typeof src === "string" ? src : "no source"})
            </a>
          ),
        }}
      >
        {children}
      </ReactMarkdown>
    </div>
  );
}
