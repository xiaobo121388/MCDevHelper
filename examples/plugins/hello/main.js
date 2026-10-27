export function activate(ctx) {
  ctx.commands.register("inspect", async () => {
    const components = await ctx.components.list();
    const settings = await ctx.settings.get();
    const current = ctx.environment.componentId ? await ctx.components.get(ctx.environment.componentId) : null;
    ctx.log.info("Read", components.length, "components");
    await ctx.storage.set("lastRead", new Date().toISOString());
    return {
      label: settings.label,
      total: components.length,
      addon: components.filter((component) => component.kind === "addon").length,
      map: components.filter((component) => component.kind === "map").length,
      material: components.filter((component) => component.kind === "material").length,
      current,
      components: components.map(({ id, name, kind, path }) => ({ id, name, kind, path })),
    };
  });
  ctx.commands.register("open", () => ctx.ui.openPanel("overview"));
}
