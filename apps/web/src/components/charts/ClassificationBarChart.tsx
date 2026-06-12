import { Bar, BarChart, CartesianGrid, Cell, LabelList, ResponsiveContainer, Tooltip, XAxis, YAxis } from "recharts";
import type { Metrics } from "../../api/types";
import { chartColors } from "../../lib/ui";

type Props = {
  metrics: Metrics;
};

export function ClassificationBarChart({ metrics }: Props) {
  const data = [
    { name: "Válidas", value: metrics.valid_requests, color: chartColors.valid },
    { name: "Respondidas", value: metrics.answered, color: chartColors.answered },
    { name: "Sin respuesta", value: metrics.unanswered, color: chartColors.unanswered },
    { name: "Ignoradas", value: metrics.ignored, color: chartColors.ignored },
    { name: "Ambiguas", value: metrics.ambiguous, color: chartColors.ambiguous },
  ];
  return (
    <section className="card chart-card">
      <h2>Distribución por clasificación</h2>
      <ResponsiveContainer width="100%" height={230}>
        <BarChart data={data} margin={{ top: 24, right: 8, left: -16, bottom: 0 }}>
          <CartesianGrid strokeDasharray="3 3" vertical={false} stroke={chartColors.grid} />
          <XAxis dataKey="name" tickLine={false} axisLine={false} tick={{ fill: chartColors.axis, fontSize: 12 }} />
          <YAxis allowDecimals={false} tickLine={false} axisLine={false} tick={{ fill: chartColors.axis, fontSize: 12 }} />
          <Tooltip cursor={{ fill: "rgba(249, 115, 22, 0.06)" }} />
          <Bar dataKey="value" radius={[8, 8, 0, 0]} maxBarSize={46}>
            <LabelList dataKey="value" position="top" fill="#44403c" fontSize={12} fontWeight={700} />
            {data.map((entry) => (
              <Cell key={entry.name} fill={entry.color} />
            ))}
          </Bar>
        </BarChart>
      </ResponsiveContainer>
    </section>
  );
}
