import {
  Bar,
  BarChart,
  CartesianGrid,
  Legend,
  Line,
  LineChart,
  ResponsiveContainer,
  Tooltip,
  XAxis,
  YAxis,
} from "recharts";
import type { RunTrendPoint } from "../../lib/aggregate";
import { chartColors } from "../../lib/ui";

type Props = {
  points: RunTrendPoint[];
};

const axisTick = { fill: chartColors.axis, fontSize: 12 };

export function ReportTrendCharts({ points }: Props) {
  return (
    <div className="report-charts">
      <section className="card chart-card">
        <h2>Tiempos de respuesta por análisis</h2>
        <ResponsiveContainer width="100%" height={240}>
          <LineChart data={points} margin={{ top: 12, right: 16, left: -8, bottom: 0 }}>
            <CartesianGrid strokeDasharray="3 3" vertical={false} stroke={chartColors.grid} />
            <XAxis dataKey="label" tickLine={false} axisLine={false} tick={axisTick} />
            <YAxis tickLine={false} axisLine={false} tick={axisTick} unit=" min" />
            <Tooltip />
            <Legend />
            <Line type="monotone" dataKey="avgFirstResponse" name="T. medio respuesta (min)" stroke={chartColors.avgResponse} strokeWidth={2.5} connectNulls dot={{ r: 4 }} />
            <Line type="monotone" dataKey="p90FirstResponse" name="P90 respuesta (min)" stroke={chartColors.p90Response} strokeWidth={2.5} connectNulls dot={{ r: 4 }} />
            <Line type="monotone" dataKey="avgResolution" name="Cierre medio (min)" stroke={chartColors.resolution} strokeWidth={2.5} connectNulls dot={{ r: 4 }} />
          </LineChart>
        </ResponsiveContainer>
        <p className="muted-note">El P90 no es agregable matemáticamente entre análisis; se muestra por análisis.</p>
      </section>
      <section className="card chart-card">
        <h2>Composición por análisis</h2>
        <ResponsiveContainer width="100%" height={240}>
          <BarChart data={points} margin={{ top: 12, right: 16, left: -16, bottom: 0 }}>
            <CartesianGrid strokeDasharray="3 3" vertical={false} stroke={chartColors.grid} />
            <XAxis dataKey="label" tickLine={false} axisLine={false} tick={axisTick} />
            <YAxis allowDecimals={false} tickLine={false} axisLine={false} tick={axisTick} />
            <Tooltip />
            <Legend />
            <Bar dataKey="answered" name="Respondidas" stackId="composition" fill={chartColors.answered} maxBarSize={42} />
            <Bar dataKey="unanswered" name="Sin respuesta" stackId="composition" fill={chartColors.unanswered} maxBarSize={42} />
            <Bar dataKey="ignored" name="Ignoradas" stackId="composition" fill={chartColors.ignored} maxBarSize={42} />
            <Bar dataKey="ambiguous" name="Ambiguas" stackId="composition" fill={chartColors.ambiguous} radius={[6, 6, 0, 0]} maxBarSize={42} />
          </BarChart>
        </ResponsiveContainer>
      </section>
    </div>
  );
}
