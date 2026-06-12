import { Cell, Pie, PieChart, ResponsiveContainer, Tooltip } from "recharts";
import type { Metrics } from "../../api/types";
import { chartColors } from "../../lib/ui";

type Props = {
  metrics: Metrics;
};

export function CompositionDonut({ metrics }: Props) {
  const total = metrics.total_threads;
  const data = [
    { name: "Válidos", value: metrics.valid_requests, color: chartColors.valid },
    { name: "Ignorados", value: metrics.ignored, color: chartColors.ignored },
    { name: "Ambiguos", value: metrics.ambiguous, color: chartColors.ambiguous },
  ].filter((slice) => slice.value > 0);

  return (
    <section className="card chart-card">
      <h2>Composición</h2>
      {total === 0 ? (
        <p className="muted-note">Aún no hay hilos analizados.</p>
      ) : (
        <div className="donut-layout">
          <div className="donut-wrap">
            <ResponsiveContainer width="100%" height={190}>
              <PieChart>
                <Pie data={data} dataKey="value" nameKey="name" innerRadius={58} outerRadius={84} paddingAngle={2} strokeWidth={0}>
                  {data.map((entry) => (
                    <Cell key={entry.name} fill={entry.color} />
                  ))}
                </Pie>
                <Tooltip />
              </PieChart>
            </ResponsiveContainer>
            <div className="donut-center">
              <strong>{total}</strong>
              <span>Total</span>
            </div>
          </div>
          <ul className="donut-legend">
            {data.map((slice) => (
              <li key={slice.name}>
                <span className="legend-dot" style={{ background: slice.color }} />
                {slice.name}
                <strong>
                  {slice.value} ({Math.round((slice.value / total) * 100)}%)
                </strong>
              </li>
            ))}
          </ul>
        </div>
      )}
    </section>
  );
}
