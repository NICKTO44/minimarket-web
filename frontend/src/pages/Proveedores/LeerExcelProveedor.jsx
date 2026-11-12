import { useEffect, useMemo, useState } from 'react';
import { api } from '../../api/api';
import {
  CAMPOS_COMPRA,
  columnasCompletas,
  detectarColumnas,
  enlacesAprendidos,
  filasDeCompra,
  hojaParaPedido,
  opcionesDeColumnas,
  reconocerFilas,
  sugerencias,
} from '../../utils/compraExcel';
import './LeerExcelProveedor.css';

// Compras por Excel con el archivo del proveedor (ver utils/compraExcel.js):
// 1) qué columna es cada dato, 2) qué producto es cada fila que no se
// reconoció sola. Lo elegido se recuerda para ese proveedor.

const soles = (n) => (n > 0 ? `S/ ${Number(n).toFixed(2)}` : 'sin costo');
const COMO = { MEMORIA: 'recordado', CODIGO: 'por código', PARTE: 'por N.° de parte', NOMBRE: 'por nombre' };

/** Columnas guardadas de este proveedor, si siguen sirviendo para este archivo. */
function columnasGuardadas(guardadas, ancho) {
  if (!guardadas || typeof guardadas !== 'object') return null;
  const limpias = {};
  for (const c of CAMPOS_COMPRA) {
    const i = guardadas[c.valor];
    if (Number.isInteger(i) && i >= 0 && i < ancho) limpias[c.valor] = i;
  }
  return columnasCompletas(limpias) ? limpias : null;
}

export default function LeerExcelProveedor({ filas, productos, fichas, proveedorId, onListo, onCancelar }) {
  const detectado = useMemo(() => detectarColumnas(filas), [filas]);
  const ancho = useMemo(() => filas.reduce((m, f) => Math.max(m, (f || []).length), 0), [filas]);
  const [cargando, setCargando] = useState(true);
  const [memoria, setMemoria] = useState(new Map());
  const [columnas, setColumnas] = useState(detectado.columnas);
  const [eligiendoColumnas, setEligiendoColumnas] = useState(false);
  // fila -> { producto } | { nuevo: true } | { omitir: true }
  const [decisiones, setDecisiones] = useState(new Map());
  const [buscando, setBuscando] = useState(null); // fila que está buscando otro producto

  // Lo aprendido de este proveedor (columnas y productos).
  useEffect(() => {
    let vivo = true;
    api
      .compraExcelProveedor(proveedorId)
      .then((r) => {
        if (!vivo) return;
        setMemoria(new Map((r.enlaces || []).map((e) => [e.clave, e.producto_id])));
        const guardadas = columnasGuardadas(r.columnas, ancho);
        // Las guardadas solo si el archivo no trae títulos que se reconozcan enteros.
        if (guardadas && !columnasCompletas(detectado.columnas)) setColumnas(guardadas);
      })
      .catch(() => {})
      .finally(() => {
        if (vivo) setCargando(false);
      });
    return () => {
      vivo = false;
    };
  }, [proveedorId, ancho, detectado.columnas]);

  const completas = columnasCompletas(columnas);
  const opciones = useMemo(() => opcionesDeColumnas(filas, detectado.filaTitulos), [filas, detectado.filaTitulos]);
  const filasCompra = useMemo(
    () => (completas ? filasDeCompra(filas, detectado.filaTitulos, columnas) : []),
    [filas, detectado.filaTitulos, columnas, completas]
  );
  const reconocidas = useMemo(() => reconocerFilas(filasCompra, productos, memoria, fichas), [filasCompra, productos, memoria, fichas]);
  const pendientes = useMemo(() => reconocidas.filter((f) => !f.producto), [reconocidas]);
  // Sugerencias de cada pendiente; con 60 % o más de parecido, la primera ya
  // viene elegida (se ve y se puede cambiar).
  const sugeridas = useMemo(() => new Map(pendientes.map((f) => [f.fila, sugerencias(f.nombre, productos)])), [pendientes, productos]);
  const efectivas = useMemo(() => {
    const m = new Map(decisiones);
    for (const f of pendientes) {
      const primera = sugeridas.get(f.fila)?.[0];
      if (!m.has(f.fila) && primera && primera.score >= 0.6) m.set(f.fila, { producto: primera.producto, sugerida: true });
    }
    return m;
  }, [decisiones, pendientes, sugeridas]);
  const faltan = pendientes.filter((f) => !efectivas.has(f.fila)).length;
  const porNombre = useMemo(() => new Map(productos.map((p) => [`${p.codigo ? `${p.codigo} — ` : ''}${p.nombre}`, p])), [productos]);

  const decidir = (fila, d) =>
    setDecisiones((actual) => {
      const nuevo = new Map(actual);
      if (d) nuevo.set(fila, d);
      else nuevo.delete(fila);
      return nuevo;
    });

  const elegirOpcion = (f, valor) => {
    setBuscando(null);
    if (valor === '') decidir(f.fila, null);
    else if (valor === 'NUEVO') decidir(f.fila, { nuevo: true });
    else if (valor === 'OMITIR') decidir(f.fila, { omitir: true });
    else if (valor === 'BUSCAR') {
      decidir(f.fila, null);
      setBuscando(f.fila);
    } else {
      const p = productos.find((x) => String(x.id) === valor);
      if (p) decidir(f.fila, { producto: p });
    }
  };

  const terminar = () => {
    const hoja = hojaParaPedido(reconocidas, efectivas);
    const enlaces = enlacesAprendidos(reconocidas, efectivas);
    onListo({ hoja, enlaces, columnas, aprendidos: enlaces.length });
  };

  const cambiarColumna = (campo, valor) => {
    const nuevas = { ...columnas };
    if (valor === '') delete nuevas[campo];
    else {
      const i = parseInt(valor, 10);
      for (const k of Object.keys(nuevas)) if (nuevas[k] === i) delete nuevas[k];
      nuevas[campo] = i;
    }
    setColumnas(nuevas);
    setDecisiones(new Map());
    setEligiendoColumnas(true);
  };

  const verColumnas = eligiendoColumnas || !completas;

  return (
    <div className="lep" role="region" aria-label="Leer el Excel del proveedor">
      <div className="lep-cabeza">
        <strong>📄 Excel del proveedor</strong>
        <button type="button" className="lep-cerrar" onClick={onCancelar} aria-label="Cancelar">
          ✕
        </button>
      </div>

      {cargando ? (
        <p className="lep-ayuda">Leyendo…</p>
      ) : verColumnas ? (
        <>
          <p className="lep-ayuda">
            {completas
              ? 'Revisa qué columna de tu Excel es cada dato.'
              : 'No reconocí todas las columnas. Elige cuál es cada dato (se recordará para este proveedor):'}
          </p>
          <div className="lep-columnas">
            {CAMPOS_COMPRA.map((c) => (
              <label key={c.valor} className="lep-columna">
                <span>
                  {c.label} <em>({c.ayuda})</em>
                </span>
                <select value={columnas[c.valor] ?? ''} onChange={(e) => cambiarColumna(c.valor, e.target.value)}>
                  <option value="">— No tiene —</option>
                  {opciones.map((o) => (
                    <option key={o.indice} value={o.indice}>
                      {o.texto}
                    </option>
                  ))}
                </select>
              </label>
            ))}
          </div>
          {!completas && <p className="lep-falta">Falta elegir la Cantidad y el Código o el Producto.</p>}
          {completas && (
            <button type="button" className="lep-boton" onClick={() => setEligiendoColumnas(false)}>
              Seguir →
            </button>
          )}
        </>
      ) : (
        <>
          <p className="lep-resumen">
            {filasCompra.length} fila{filasCompra.length === 1 ? '' : 's'} con cantidad ·{' '}
            <strong>{reconocidas.length - pendientes.length} reconocida{reconocidas.length - pendientes.length === 1 ? '' : 's'}</strong>
            {pendientes.length > 0 && (
              <>
                {' '}
                · <strong className="lep-por-revisar">{pendientes.length} por revisar</strong>
              </>
            )}{' '}
            ·{' '}
            <button type="button" className="lep-enlace" onClick={() => setEligiendoColumnas(true)}>
              cambiar columnas
            </button>
          </p>

          {filasCompra.length === 0 && <p className="lep-falta">No encontré filas con cantidad. Revisa las columnas.</p>}

          {reconocidas.length - pendientes.length > 0 && (
            <details className="lep-reconocidas">
              <summary>Ver las reconocidas</summary>
              <ul>
                {reconocidas
                  .filter((f) => f.producto)
                  .map((f) => (
                    <li key={f.fila}>
                      <span className="lep-excel">{f.nombre || f.codigo}</span> → <strong>{f.producto.nombre}</strong>{' '}
                      <em>({COMO[f.como]})</em>
                    </li>
                  ))}
              </ul>
            </details>
          )}

          {pendientes.length > 0 && (
            <>
              <p className="lep-ayuda">
                Estos no los reconocí. Dime qué producto es cada uno: la próxima compra de este proveedor ya entran solos.
              </p>
              <div className="lep-pendientes">
                {pendientes.map((f) => {
                  const d = efectivas.get(f.fila);
                  const sug = sugeridas.get(f.fila) || [];
                  const valor = d?.producto ? String(d.producto.id) : d?.nuevo ? 'NUEVO' : d?.omitir ? 'OMITIR' : buscando === f.fila ? 'BUSCAR' : '';
                  const elegidoFuera = d?.producto && !sug.some((s) => s.producto.id === d.producto.id);
                  return (
                    <div key={f.fila} className={`lep-pendiente${d ? ' lep-decidida' : ''}`}>
                      <div className="lep-pendiente-excel">
                        <span className="lep-fila">fila {f.fila}</span>
                        {f.codigo && <span className="lep-codigo">{f.codigo}</span>}
                        <strong>{f.nombre || '(sin nombre)'}</strong>
                        <span className="lep-cant">
                          {f.cantidad} × {soles(f.costo)}
                        </span>
                      </div>
                      <select
                        value={valor}
                        onChange={(e) => elegirOpcion(f, e.target.value)}
                        aria-label={`Qué producto es ${f.nombre || f.codigo}`}
                        className={d ? '' : 'lep-sin-decidir'}
                      >
                        <option value="">Elige qué producto es…</option>
                        {sug.map((s) => (
                          <option key={s.producto.id} value={s.producto.id}>
                            {s.producto.nombre} ({Math.round(s.score * 100)} % parecido)
                          </option>
                        ))}
                        {elegidoFuera && <option value={d.producto.id}>{d.producto.nombre}</option>}
                        <option value="BUSCAR">🔎 Buscar otro producto…</option>
                        <option value="NUEVO">➕ Es nuevo: crearlo</option>
                        <option value="OMITIR">✕ No comprarlo</option>
                      </select>
                      {d?.sugerida && <span className="lep-sugerida">Elegido por parecido: revísalo.</span>}
                      {buscando === f.fila && (
                        <input
                          className="lep-buscar"
                          list="lep-productos"
                          autoFocus
                          placeholder="Escribe el nombre o código y elígelo"
                          onChange={(e) => {
                            const p = porNombre.get(e.target.value);
                            if (p) {
                              decidir(f.fila, { producto: p });
                              setBuscando(null);
                            }
                          }}
                        />
                      )}
                    </div>
                  );
                })}
              </div>
              <datalist id="lep-productos">
                {[...porNombre.keys()].map((k) => (
                  <option key={k} value={k} />
                ))}
              </datalist>
              {faltan > 0 && (
                <button
                  type="button"
                  className="lep-enlace"
                  onClick={() =>
                    setDecisiones((actual) => {
                      const nuevo = new Map(actual);
                      pendientes.forEach((f) => {
                        if (!efectivas.has(f.fila)) nuevo.set(f.fila, { nuevo: true });
                      });
                      return nuevo;
                    })
                  }
                >
                  {faltan === 1 ? 'El que falta es nuevo: crearlo' : `Los ${faltan} que faltan son nuevos: crearlos`}
                </button>
              )}
            </>
          )}

          <div className="lep-acciones">
            <button type="button" className="lep-boton-sec" onClick={onCancelar}>
              Cancelar
            </button>
            <button type="button" className="lep-boton" disabled={faltan > 0 || filasCompra.length === 0} onClick={terminar}>
              {faltan > 0 ? `Falta decidir ${faltan}` : 'Agregar a la compra'}
            </button>
          </div>
        </>
      )}
    </div>
  );
}
