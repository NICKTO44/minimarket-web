import { useEffect, useState } from 'react';
import { api } from '../../api/api';
import { hoyLima } from '../../utils/formato';
import { TIPO_XLSX, escribirXlsx } from '../../utils/hojas';
import { descargarArchivo } from '../../utils/pdfTicket';
import './VentasEditorial.css';

// Librería (módulo "Ficha del libro"): cuánto se vendió de cada editorial en
// un rango de fechas. Solo cuenta los productos con ficha de libro y las
// ventas completadas, igual que el resto de reportes.
const RANGOS = [
  { label: 'Hoy', dias: 0 },
  { label: '7 días', dias: 6 },
  { label: '30 días', dias: 29 },
];

const soles = (n) => `S/ ${Number(n || 0).toLocaleString('es-PE', { minimumFractionDigits: 2, maximumFractionDigits: 2 })}`;
const cantidad = (n) => Number(n || 0).toLocaleString('es-PE', { maximumFractionDigits: 3 });

export default function VentasEditorial() {
  const [desde, setDesde] = useState(hoyLima(-29));
  const [hasta, setHasta] = useState(hoyLima());
  // Lo último que respondió el servidor y para qué fechas: si no son las
  // elegidas ahora, está cargando.
  const [resultado, setResultado] = useState({ clave: null, filas: [], error: null });
  const clave = `${desde}|${hasta}`;
  const cargando = resultado.clave !== clave;
  const filas = cargando ? [] : resultado.filas;
  const error = cargando ? null : resultado.error;

  useEffect(() => {
    let vivo = true;
    api
      .librosVentasPorEditorial(desde, hasta)
      .then((lista) => vivo && setResultado({ clave: `${desde}|${hasta}`, filas: lista || [], error: null }))
      .catch((e) => vivo && setResultado({ clave: `${desde}|${hasta}`, filas: [], error: e.message }));
    return () => {
      vivo = false;
    };
  }, [desde, hasta]);

  const total = filas.reduce((s, f) => s + Number(f.total || 0), 0);
  const unidades = filas.reduce((s, f) => s + Number(f.unidades || 0), 0);

  const descargar = () => {
    const datos = [
      ['Editorial', 'Títulos vendidos', 'Unidades', 'Total (S/)', '% del total'],
      ...filas.map((f) => [f.editorial, f.titulos, Number(f.unidades), Number(f.total), total > 0 ? Math.round((f.total / total) * 1000) / 10 : 0]),
      ['TOTAL', '', unidades, Math.round(total * 100) / 100, 100],
    ];
    descargarArchivo(escribirXlsx(datos, { hoja: 'Por editorial', anchos: [30, 16, 12, 14, 12] }), `Ventas-por-editorial-${desde}-a-${hasta}.xlsx`, {
      tipo: TIPO_XLSX,
    });
  };

  return (
    <div className="ved">
      <div className="ved-filtros">
        <div className="ved-rangos">
          {RANGOS.map((r) => (
            <button key={r.label} type="button" onClick={() => { setDesde(hoyLima(-r.dias)); setHasta(hoyLima()); }}>
              {r.label}
            </button>
          ))}
        </div>
        <label className="ved-fecha">
          Desde
          <input type="date" value={desde} max={hasta} onChange={(e) => setDesde(e.target.value)} />
        </label>
        <label className="ved-fecha">
          Hasta
          <input type="date" value={hasta} min={desde} max={hoyLima()} onChange={(e) => setHasta(e.target.value)} />
        </label>
        <button type="button" className="ved-excel" disabled={filas.length === 0} onClick={descargar}>
          ⬇ Excel
        </button>
      </div>

      {error && <p className="ved-error">{error}</p>}
      {cargando ? (
        <p className="ved-vacio">Cargando…</p>
      ) : filas.length === 0 ? (
        <p className="ved-vacio">No hay ventas de libros en estas fechas. Solo cuentan los productos con ficha de libro.</p>
      ) : (
        <div className="ved-caja">
          <table className="ved-tabla">
            <thead>
              <tr>
                <th scope="col">Editorial</th>
                <th scope="col" className="ved-num">Títulos</th>
                <th scope="col" className="ved-num">Unidades</th>
                <th scope="col" className="ved-num">Total</th>
                <th scope="col" className="ved-barra-col">% del total</th>
              </tr>
            </thead>
            <tbody>
              {filas.map((f) => {
                const pct = total > 0 ? (f.total / total) * 100 : 0;
                return (
                  <tr key={f.editorial}>
                    <td>{f.editorial}</td>
                    <td className="ved-num">{f.titulos}</td>
                    <td className="ved-num">{cantidad(f.unidades)}</td>
                    <td className="ved-num ved-fuerte">{soles(f.total)}</td>
                    <td className="ved-barra-col">
                      <span className="ved-barra" aria-hidden="true">
                        <span style={{ width: `${pct}%` }} />
                      </span>
                      <small>{pct.toFixed(1)} %</small>
                    </td>
                  </tr>
                );
              })}
            </tbody>
            <tfoot>
              <tr>
                <th scope="row">Total</th>
                <td />
                <td className="ved-num">{cantidad(unidades)}</td>
                <td className="ved-num ved-fuerte">{soles(total)}</td>
                <td />
              </tr>
            </tfoot>
          </table>
        </div>
      )}
    </div>
  );
}
