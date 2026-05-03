import "./globals.css";

export const metadata = {
  title: "cloak, per-directory profiles for LLM CLIs",
  description:
    "A focused landing page for cloak, a Rust CLI that isolates LLM profiles by directory.",
};

export default function RootLayout({ children }) {
  return (
    <html lang="pt-BR">
      <body className="overflow-x-hidden bg-cloak-ink font-sans text-cloak-paper antialiased">
        {children}
      </body>
    </html>
  );
}
