import { useEffect, useState } from 'react';
import { api } from '../../api/api';
import BuscadorProducto from '../../components/BuscadorProducto';
import { escribirXlsx, TIPO_XLSX } from '../../utils/hojas';
import { descargarArchivo } from '../../utils/pdfTicket';
import { formatoCantidad } from '../../utils/medidas';
import { filasDelKardex } from '../../utils/kardex';


const fechaHora = (f) => {
  const [d, h] = String(f || '').split(' ');
  return `${(d || '').split('-').reverse().join('/')} ${(h || '').slice(0, 5)}`;
};
const num = (n) => (n ? formatoCantidad(n) : '');

/** Kárdex de un producto: todas sus entradas y salidas con el saldo. */
export default function Kardex() {
  const [productos, setProductos] = useState([]);
  const [elegido, setElegido] = useState('');
  const [desde, setDesde] = useState('');
  const [hasta, setHasta] = useState('');
  // Lo último que llegó del servidor, con la consulta a la que responde.
  const [respuesta, setRespuesta] = useState({ clave: '', kardex: null, error: null });
  const [errorProductos, setErrorProductos] = useState(null);
  const clave = `${elegido}|${desde}|${hasta}`;
  const cargando = !!elegido && respuesta.clave !== clave;
  // Solo el del producto elegido (al cambiar de producto no queda el anterior).
  const kardex = respuesta.kardex && String(respuesta.kardex.producto_id) === String(elegido) ? respuesta.kardex : null;
  const error = errorProductos || (respuesta.clave === clave ? respuesta.error : null);

  useEffect(() => {
    api
      .productos()
      .then((p) => setProductos((p || []).filter((x) => x.controla_stock !== false)))
      .catch((e) => setErrorProductos(e.message));
  }, []);

  useEffect(() => {
    if (!elegido) return undefined;
    let vigente = true;
    const consulta = `${elegido}|${desde}|${hasta}`;
    api
      .kardex(elegido, desde, hasta)
      .then((k) => vigente && setRespuesta({ clave: consulta, kardex: k, error: null }))
      .catch((e) => vigente && setRespuesta((r) => ({ clave: consulta, kardex: r.kardex, error: e.message })));
    return () => {
      vigente = false;
    };
  }, [elegido, desde, hasta]);

  // Filas a mostrar, de la más nueva a la más antigua. Donde el stock con que
  // empieza un movimiento no es el saldo del anterior hubo cambios que no se
  // anotaron (ventas o importaciones de antes de este módulo): se muestra la
  // diferencia como "Sin registro". Lo mismo entre el último saldo y el stock
  // de hoy (solo sin fecha "hasta"). Al final, el saldo inicial.
  const filas = filasDelKardex(kardex, { hastaHoy: !hasta });

  const exportar = () => {
    const filas = [
      [`Kárdex: ${kardex.nombre}${kardex.codigo ? ` (${kardex.codigo})` : ''}`],
      [`Stock actual: ${kardex.stock_actual}`],
      [],
      ['Fecha', 'Tipo', 'Referencia', 'Entrada', 'Salida', 'Saldo', 'Usuario', 'Motivo'],
      ...[...filas]
        .reverse()
        .map((f) => [f.fecha ? fechaHora(f.fecha) : '', f.etiqueta, f.referencia || '', f.entrada || '', f.salida || '', f.saldo, f.usuario || '', f.motivo || '']),
    ];
    descargarArchivo(escribirXlsx(filas, { hoja: 'Kardex', anchos: [17, 12, 22, 10, 10, 10, 20, 28] }), `Kardex ${kardex.codigo || kardex.nombre}.xlsx`, {
      tipo: TIPO_XLSX,
    });
  };

  return (
    <div className="kx">
      <p className="sl-subtitulo">
        Elige un producto para ver todas sus entradas y salidas (compras, ventas, devoluciones, mermas y ajustes) con el stock que quedó
        después de cada una.
      </p>
      <div className="kx-filtros">
        <div className="kx-buscador">
          <BuscadorProducto productos={productos} valor={elegido} onElegir={setElegido} />
        </div>
        <label>
          Desde
          <input type="date" value={desde} onChange={(e) => setDesde(e.target.value)} />
        </label>
        <label>
          Hasta
          <input type="date" value={hasta} onChange={(e) => setHasta(e.target.value)} />
        </label>
        {kardex && elegido && kardex.movimientos.length > 0 && (
          <button type="button" className="kx-exportar" onClick={exportar}>
            ⬇ Excel
          </button>
        )}
      </div>

      {error && <p className="sl-mensaje sl-mensaje-error">{error}</p>}
      {!elegido ? null : cargando && !kardex ? (
        <p className="sl-cargando">Cargando...</p>
      ) : kardex ? (
        <>
          <div className="kx-resumen">
            <span>
              <b>{kardex.nombre}</b>
              {kardex.codigo && <span className="aj-codigo">{kardex.codigo}</span>}
            </span>
            <span>
              Stock actual: <b>{formatoCantidad(kardex.stock_actual)}</b>
            </span>
          </div>
          {kardex.movimientos.length === 0 ? (
            <p className="sl-vacio">Sin movimientos {desde || hasta ? 'en esas fechas' : 'todavía'}.</p>
          ) : (
            <div className="kx-tabla-caja">
              <table className="aj-tabla kx-tabla">
                <thead>
                  <tr>
                    <th>Fecha</th>
                    <th>Tipo</th>
                    <th>Referencia</th>
                    <th className="kx-num">Entrada</th>
                    <th className="kx-num">Salida</th>
                    <th className="kx-num">Saldo</th>
                    <th>Usuario</th>
                    <th>Motivo</th>
                  </tr>
                </thead>
                <tbody>
                  {filas.map((f) => (
                    <tr key={f.clave} className={f.hueco ? 'kx-hueco' : undefined}>
                      <td>{f.fecha ? fechaHora(f.fecha) : '—'}</td>
                      <td>
                        <span className={`kx-tipo${f.tipo ? ` kx-tipo-${f.tipo.toLowerCase()}` : ''}`}>{f.etiqueta}</span>
                      </td>
                      <td>{f.referencia || ''}</td>
                      <td className="kx-num aj-sube">{num(f.entrada)}</td>
                      <td className="kx-num aj-baja">{num(f.salida)}</td>
                      <td className="kx-num">
                        <b>{formatoCantidad(f.saldo)}</b>
                      </td>
                      <td>{f.usuario || ''}</td>
                      <td>{f.motivo || ''}</td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          )}
        </>
      ) : null}
    </div>
  );
}
