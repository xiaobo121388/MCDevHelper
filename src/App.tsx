import { Boxes } from "lucide-react";

export function App() {
  return (
    <main className="grid min-h-screen place-items-center bg-background text-foreground">
      <section className="flex max-w-md flex-col items-center gap-4 px-6 text-center">
        <div className="grid size-14 place-items-center rounded-2xl bg-primary text-primary-foreground shadow-lg">
          <Boxes aria-hidden="true" />
        </div>
        <div>
          <p className="text-sm font-medium tracking-[0.22em] text-muted-foreground">MCDEVHELPER</p>
          <h1 className="mt-2 text-3xl font-semibold">组件管理，简单一点</h1>
        </div>
        <p className="text-sm leading-6 text-muted-foreground">MCDH 正在准备你的本地创作空间。</p>
      </section>
    </main>
  );
}

