import { useEffect, useMemo, useState } from 'react';
import { api } from '../../api/api';
import { hoyLima } from '../../utils/formato';
import { escribirLibro, TIPO_XLSX } from '../../utils/hojas';
import { descargarArchivo } from '../../utils/pdfTicket';
import { confirmar } from '../../utils/confirmar';
import './Ganancias.css';
import './MargenPlatos.css';

// Margen por plato (módulo COSTEO, solo restaurante y administrador).
// El margen se mide sobre el precio SIN IGV. El costo de cada plato sale de
// su receta, de un costo fijo, de su precio de compra o de un % aproximado
// (handlers/costeo.rs). Cuatro vistas: el mes, la carta, los insumos y los
// ajustes. Todo se descarga en un Excel de varias hojas.

const MESES = ['enero', 'febrero', 'marzo', 'abril', 'mayo', 'junio', 'julio', 'agosto', 'septiembre', 'octubre', 'noviembre', 'diciembre'];
const nombreMes = (clave) => `${MESES[Number(clave.slice(5, 7)) - 1]} ${clave.slice(0, 4)}`;
const mayuscula = (t) => t.charAt(0).toUpperCase() + t.slice(1);
function moverMes(clave, meses) {
  const total = Number(clave.slice(0, 4)) * 12 + Number(clave.slice(5, 7)) - 1 + meses;
  return `${Math.floor(total / 12)}-${String((total % 12) + 1).padStart(2, '0')}`;
}

const numero = (v) => (Number(v) || 0).toLocaleString('es-PE', { minimumFractionDigits: 2, maximumFractionDigits: 2 });
const soles = (v) => (v < 0 ? `− S/ ${numero(-v)}` : `S/ ${numero(v)}`);
const pct = (v) => (v == null ? '—' : `${Number(v).toLocaleString('es-PE', { maximumFractionDigits: 1 })} %`);
const cant = (v) => (Number(v) || 0).toLocaleString('es-PE', { maximumFractionDigits: 3 });
const r2 = (v) => Math.round(v * 100) / 100;

const METODOS = {
  RECETA: { texto: 'Receta', exacto: true },
  FIJO: { texto: 'Costo fijo', exacto: true },
  COMPRA: { texto: 'Precio de compra', exacto: true },
  CATEGORIA: { texto: '% de su categoría', exacto: false },
  GENERAL: { texto: '% aproximado', exacto: false },
};
const REGIMENES = {
  GENERAL: 'Cobro IGV (boleta o factura con IGV)',
  RUS: 'Nuevo RUS (no cobro IGV)',
  NO_SE: 'No sé todavía',
};
const UNIDADES = { KG: { compra: 'kilo', uso: 'g' }, LITRO: { compra: 'litro', uso: 'ml' }, UNIDAD: { compra: 'unidad', uso: 'unid.' } };
const factor = (unidad) => (unidad === 'UNIDAD' ? 1 : 1000);

/** ¿Recupera el IGV de lo que compra? */
const recupera = (config) => config && config.regimen !== 'RUS' && config.compras_con_factura;
/** Lo que pesa en el costo: lo pagado, sin el 18 % si lo recupera. */
const costoEfectivo = (pagado, config, conIgv = true) => (recupera(config) && conIgv ? pagado / 1.18 : pagado);

function Etiqueta({ metodo }) {
  const m = METODOS[metodo] || METODOS.GENERAL;
  return <span className={`mp-metodo${m.exacto ? '' : ' aprox'}`}>{m.exacto ? m.texto : `aprox. · ${m.texto}`}</span>;
}

export default function MargenPlatos() {
  const [vista, setVista] = useState('MES');
  const [config, setConfig] = useState(null);
  const [error, setError] = useState(null);
  const [descargando, setDescargando] = useState(false);
  // Para que la carta y el mes se recarguen después de cambiar algo.
  const [version, setVersion] = useState(0);
  const cambio = () => setVersion((v) => v + 1);

  useEffect(() => {
    api
      .costeoConfig()
      .then(setConfig)
      .catch((e) => setError(e.message));
  }, [version]);

  const descargar = async (mes) => {
    setDescargando(true);
    try {
      const [reporte, carta, insumos] = await Promise.all([api.costeoReporte(mes), api.costeoCarta(), api.costeoInsumos()]);
      const bytes = libroExcel(reporte, carta, insumos);
      descargarArchivo(bytes, `Margen-por-plato-${reporte.mes}.xlsx`, { tipo: TIPO_XLSX });
    } catch (e) {
      setError(e.message);
    } finally {
      setDescargando(false);
    }
  };

  if (!config) return error ? <p className="rep-mensaje">{error}</p> : <p className="rep-cargando">Cargando...</p>;

  return (
    <div className="mp">
      <div className="mp-barra">
        <div className="gan-conmutador" role="tablist" aria-label="Ver">
          {[
            ['MES', 'Ganancia del mes'],
            ['CARTA', 'Costo de cada plato'],
            ['INSUMOS', 'Insumos'],
            ['AJUSTES', 'Ajustes'],
          ].map(([valor, texto]) => (
            <button key={valor} type="button" role="tab" aria-selected={vista === valor} className={vista === valor ? 'activo' : ''} onClick={() => setVista(valor)}>
              {texto}
            </button>
          ))}
        </div>
      </div>

      {error && (
        <p className="rep-mensaje" role="alert">
          {error}
        </p>
      )}

      {config.regimen === 'NO_SE' && vista !== 'AJUSTES' && (
        <div className="gan-aviso">
          <p>
            <strong>Falta un dato para que el margen salga exacto:</strong> ¿cobras IGV o estás en el Nuevo RUS? Mientras no lo
            elijas se calcula como si cobraras IGV ({pct(config.tasa_igv)}).
          </p>
          <button type="button" onClick={() => setVista('AJUSTES')}>
            Elegir
          </button>
        </div>
      )}

      {vista === 'MES' && <VistaMes version={version} onDescargar={descargar} descargando={descargando} onVerCarta={() => setVista('CARTA')} />}
      {vista === 'CARTA' && <VistaCarta version={version} config={config} onCambio={cambio} onDescargar={() => descargar()} descargando={descargando} />}
      {vista === 'INSUMOS' && <VistaInsumos config={config} onCambio={cambio} />}
      {vista === 'AJUSTES' && (
        <VistaAjustes
          config={config}
          onGuardado={(nueva) => {
            setConfig(nueva);
            cambio();
            setVista('CARTA');
          }}
        />
      )}
    </div>
  );
}

// ------------------------------------------------------------------ El mes

function VistaMes({ version, onDescargar, descargando, onVerCarta }) {
  const mesActual = hoyLima().slice(0, 7);
  const [mes, setMes] = useState(mesActual);
  const [datos, setDatos] = useState(null);
  const [cargando, setCargando] = useState(true);
  const [error, setError] = useState(null);
  const [orden, setOrden] = useState('ganancia');
  const [soloBajos, setSoloBajos] = useState(false);

  useEffect(() => {
    let vigente = true;
    api
      .costeoReporte(mes)
      .then((r) => {
        if (!vigente) return;
        setDatos(r);
        setError(null);
      })
      .catch((e) => vigente && setError(e.message))
      .finally(() => vigente && setCargando(false));
    return () => {
      vigente = false;
    };
  }, [mes, version]);

  const platos = useMemo(() => {
    const lista = (datos?.platos || []).filter((p) => !soloBajos || p.bajo_objetivo);
    const valor = (p) => (p[orden] == null ? -Infinity : p[orden]);
    return [...lista].sort((a, b) => valor(b) - valor(a));
  }, [datos, orden, soloBajos]);

  const elegirMes = (nuevo) => {
    if (nuevo === mes || nuevo > mesActual) return;
    setCargando(true);
    setMes(nuevo);
  };

  if (!datos) return error ? <p className="rep-mensaje">{error}</p> : <p className="rep-cargando">Cargando...</p>;
  const s = datos.resumen;
  const cobraIgv = datos.config.regimen !== 'RUS';
  const objetivo = datos.config.margen_objetivo;
  const sinVentas = !(Math.abs(s.ventas) > 0.005) && datos.platos.length === 0;

  return (
    <div className={`gan${cargando ? ' gan-actualizando' : ''}`}>
      <div className="gan-mes">
        <button type="button" onClick={() => elegirMes(moverMes(mes, -1))} aria-label="Mes anterior">
          ‹
        </button>
        <strong>{mayuscula(nombreMes(datos.mes))}</strong>
        <button type="button" onClick={() => elegirMes(moverMes(mes, 1))} disabled={mes >= mesActual} aria-label="Mes siguiente">
          ›
        </button>
        {mes !== mesActual && (
          <button type="button" className="gan-mes-hoy" onClick={() => elegirMes(mesActual)}>
            Ir a este mes
          </button>
        )}
        <button type="button" className="mp-descargar" onClick={() => onDescargar(mes)} disabled={descargando}>
          {descargando ? 'Preparando…' : 'Descargar Excel'}
        </button>
      </div>

      {error && <p className="rep-mensaje">{error}</p>}

      {sinVentas ? (
        <p className="rep-sin-resultados mp-vacio">No hay ventas en {nombreMes(datos.mes)}.</p>
      ) : (
        <>
          {s.platos_bajo_objetivo > 0 && (
            <div className="mp-alerta" role="status">
              <p>
                <strong>
                  {s.platos_bajo_objetivo === 1 ? '1 plato gana' : `${s.platos_bajo_objetivo} platos ganan`} menos del margen que
                  quieres.
                </strong>{' '}
                Están marcados en rojo. En “Costo de cada plato” ves el precio que necesitarías.
              </p>
              <button type="button" onClick={() => setSoloBajos((v) => !v)}>
                {soloBajos ? 'Ver todos' : 'Ver solo esos'}
              </button>
            </div>
          )}

          <div className="rep-tarjetas gan-tarjetas">
            <div className="rep-tarjeta">
              <span className="rep-tarjeta-label">Vendiste</span>
              <strong className="rep-tarjeta-valor">{soles(s.ventas)}</strong>
              <span className="rep-tarjeta-sub">{cobraIgv ? `${soles(s.ventas_sin_igv)} sin IGV` : 'Nuevo RUS: sin IGV'}</span>
            </div>
            <div className="rep-tarjeta">
              <span className="rep-tarjeta-label">Costo de los platos</span>
              <strong className="rep-tarjeta-valor">{soles(s.costo)}</strong>
              <span className="rep-tarjeta-sub">Food cost: {pct(s.food_cost)}</span>
            </div>
            <div className="rep-tarjeta rep-tarjeta-destacada">
              <span className="rep-tarjeta-label">Ganancia de los platos</span>
              <strong className="rep-tarjeta-valor">{soles(s.ganancia_platos)}</strong>
              <span className="rep-tarjeta-sub">Margen {pct(s.margen_platos)} · quieres {pct(objetivo)}</span>
            </div>
            <div className="rep-tarjeta">
              <span className="rep-tarjeta-label">Ganancia neta</span>
              <strong className={`rep-tarjeta-valor${s.ganancia_neta < 0 ? ' gan-perdida' : ''}`}>{soles(s.ganancia_neta)}</strong>
              <span className="rep-tarjeta-sub">Margen neto {pct(s.margen_neto)}</span>
            </div>
          </div>

          {/* La cuenta completa, de lo cobrado a lo que queda. */}
          <div className="rep-seccion">
            <h2>Cómo se llega a tu ganancia</h2>
            <div className="mp-cuenta">
              <Fila texto="Lo que cobraste (platos y envíos)" valor={s.ventas} />
              {cobraIgv && <Fila texto={`− IGV (${pct(datos.tasa_igv)}, es de SUNAT)`} valor={-s.igv} />}
              {cobraIgv && <Fila texto="= Ventas sin IGV" valor={s.ventas_sin_igv} fuerte />}
              {s.envios_sin_igv > 0.005 && <Fila texto="de eso, cobros de envío del delivery (no llevan insumos)" valor={s.envios_sin_igv} suave />}
              <Fila texto="− Costo de los platos vendidos" valor={-s.costo} />
              <Fila texto="= Ganancia bruta" valor={s.ganancia_bruta} fuerte />
              <Fila
                texto={`− Gastos del mes${s.gastos_fuente === 'ESTIMADOS' ? ' (aproximados, de Ajustes)' : s.gastos_fuente === 'NINGUNO' ? ' (no hay)' : ' (registrados en Gastos)'}`}
                valor={-s.gastos}
              />
              <Fila texto="= Ganancia neta" valor={s.ganancia_neta} total />
            </div>
            <ul className="gan-notas">
              {s.punto_equilibrio != null && (
                <li>
                  Para cubrir tus gastos necesitas vender <strong>{soles(s.punto_equilibrio)}</strong> en el mes
                  {s.ventas >= s.punto_equilibrio ? ' — ya lo pasaste.' : ` — te faltan ${soles(s.punto_equilibrio - s.ventas)}.`}
                </li>
              )}
              {s.gastos_fuente === 'NINGUNO' && (
                <li>Sin gastos no hay ganancia neta real: anótalos en la pantalla Gastos (alquiler, luz, sueldos) o pon un aproximado en Ajustes.</li>
              )}
              {s.costo_estimado > 0.05 && (
                <li>
                  El {pct(s.costo_estimado)} del costo es <strong>aproximado</strong> (platos sin receta ni costo).{' '}
                  <button type="button" className="mp-enlace" onClick={onVerCarta}>
                    Completa sus costos
                  </button>{' '}
                  para que el margen sea exacto.
                </li>
              )}
              {s.devoluciones > 0.005 && <li>Ya están restadas las devoluciones del mes ({soles(s.devoluciones)}).</li>}
              <li>El costo de cada plato es el de hoy (si cambias una receta o un precio de insumo, cambia también en meses pasados).</li>
            </ul>
          </div>

          <div className="rep-seccion">
            <div className="gan-cabecera">
              <h2>Plato por plato</h2>
              <div className="gan-conmutador" role="tablist" aria-label="Ordenar por">
                {[
                  ['ganancia', 'Ganancia'],
                  ['margen', 'Margen'],
                  ['cantidad', 'Cantidad'],
                ].map(([valor, texto]) => (
                  <button key={valor} type="button" className={orden === valor ? 'activo' : ''} onClick={() => setOrden(valor)}>
                    {texto}
                  </button>
                ))}
              </div>
            </div>
            <div className="rep-tabla-wrapper">
              <table className="rep-tabla mp-tabla">
                <thead>
                  <tr>
                    <th>Plato</th>
                    <th className="gan-num">Cant.</th>
                    <th className="gan-num mp-opcional">Precio prom.</th>
                    <th className="gan-num mp-opcional">{cobraIgv ? 'Vendido sin IGV' : 'Vendido'}</th>
                    <th className="gan-num mp-opcional">Costo c/u</th>
                    <th className="gan-num mp-opcional">Costo</th>
                    <th className="gan-num">Ganancia</th>
                    <th className="gan-num">Margen</th>
                    <th className="gan-num mp-opcional">Aporte</th>
                  </tr>
                </thead>
                <tbody>
                  {platos.map((p) => (
                    <tr key={p.producto_id} className={p.bajo_objetivo ? 'mp-bajo' : ''}>
                      <td>
                        <strong>{p.nombre}</strong>
                        <small className="gan-categoria">
                          {p.categoria} · <Etiqueta metodo={p.metodo} />
                        </small>
                        <small className="gan-categoria mp-solo-movil">
                          Vendido {soles(p.venta_sin_igv)} · costo {soles(p.costo)}
                        </small>
                      </td>
                      <td className="gan-num">{cant(p.cantidad)}</td>
                      <td className="gan-num mp-opcional">{p.precio_promedio == null ? '—' : soles(p.precio_promedio)}</td>
                      <td className="gan-num mp-opcional">{soles(p.venta_sin_igv)}</td>
                      <td className="gan-num mp-opcional">{p.costo_unitario == null ? '—' : soles(p.costo_unitario)}</td>
                      <td className="gan-num mp-opcional">{soles(p.costo)}</td>
                      <td className={`gan-num gan-fuerte${p.ganancia < 0 ? ' gan-perdida' : ''}`}>{soles(p.ganancia)}</td>
                      <td className={`gan-num${p.bajo_objetivo ? ' mp-rojo' : ''}`} title={`Quieres ${pct(p.objetivo)}`}>
                        {pct(p.margen)}
                      </td>
                      <td className="gan-num mp-opcional">{pct(p.participacion)}</td>
                    </tr>
                  ))}
                  {platos.length === 0 && (
                    <tr>
                      <td colSpan={9} className="rep-sin-resultados">
                        {soloBajos ? 'Ningún plato está debajo de tu margen objetivo.' : 'Sin platos vendidos.'}
                      </td>
                    </tr>
                  )}
                </tbody>
              </table>
            </div>
            <p className="gan-pie">
              Margen = ganancia ÷ lo vendido {cobraIgv ? 'sin IGV' : ''}. Aporte = qué parte de la ganancia de todos los platos dejó
              cada uno.
            </p>
          </div>
        </>
      )}
    </div>
  );
}

function Fila({ texto, valor, fuerte, total, suave }) {
  return (
    <div className={`mp-cuenta-fila${fuerte ? ' fuerte' : ''}${total ? ' total' : ''}${suave ? ' suave' : ''}`}>
      <span>{texto}</span>
      <strong className={total && valor < 0 ? 'gan-perdida' : ''}>{soles(valor)}</strong>
    </div>
  );
}

// ------------------------------------------------------------------ La carta

function VistaCarta({ version, config, onCambio, onDescargar, descargando }) {
  const [carta, setCarta] = useState(null);
  const [error, setError] = useState(null);
  const [filtro, setFiltro] = useState('TODOS');
  const [buscar, setBuscar] = useState('');
  const [editando, setEditando] = useState(null);

  useEffect(() => {
    api
      .costeoCarta()
      .then(setCarta)
      .catch((e) => setError(e.message));
  }, [version]);

  const platos = useMemo(() => {
    const texto = buscar.trim().toLowerCase();
    return (carta?.platos || []).filter(
      (p) =>
        (filtro === 'TODOS' || (filtro === 'BAJOS' && p.bajo_objetivo) || (filtro === 'APROX' && !p.exacto)) &&
        (!texto || p.nombre.toLowerCase().includes(texto) || p.categoria.toLowerCase().includes(texto))
    );
  }, [carta, filtro, buscar]);

  if (!carta) return error ? <p className="rep-mensaje">{error}</p> : <p className="rep-cargando">Cargando...</p>;
  const cobraIgv = carta.config.regimen !== 'RUS';

  return (
    <div>
      <div className="mp-filtros">
        <input type="search" placeholder="Buscar plato o categoría" value={buscar} onChange={(e) => setBuscar(e.target.value)} aria-label="Buscar plato" />
        <div className="gan-conmutador" role="tablist" aria-label="Mostrar">
          {[
            ['TODOS', `Todos (${carta.platos.length})`],
            ['BAJOS', `Bajo tu margen (${carta.bajo_objetivo})`],
            ['APROX', `Costo aproximado (${carta.estimados})`],
          ].map(([valor, texto]) => (
            <button key={valor} type="button" className={filtro === valor ? 'activo' : ''} onClick={() => setFiltro(valor)}>
              {texto}
            </button>
          ))}
        </div>
        <button type="button" className="mp-descargar" onClick={onDescargar} disabled={descargando}>
          {descargando ? 'Preparando…' : 'Descargar Excel'}
        </button>
      </div>
      <p className="mp-ayuda">
        Toca un plato para ponerle su receta o lo que te cuesta. Margen que quieres: <strong>{pct(carta.config.margen_objetivo)}</strong>{' '}
        {cobraIgv ? `· precios con IGV ${pct(carta.tasa_igv)}` : '· Nuevo RUS (sin IGV)'}.
      </p>
      <div className="rep-tabla-wrapper">
        <table className="rep-tabla mp-tabla">
          <thead>
            <tr>
              <th>Plato</th>
              <th className="gan-num mp-opcional">Precio</th>
              {cobraIgv && <th className="gan-num mp-opcional">Sin IGV</th>}
              <th className="gan-num">Te cuesta</th>
              <th className="gan-num mp-opcional">Ganas</th>
              <th className="gan-num">Margen</th>
              <th className="gan-num mp-opcional">Precio sugerido</th>
            </tr>
          </thead>
          <tbody>
            {platos.map((p) => (
              <tr key={p.producto_id} className={`mp-fila-boton${p.bajo_objetivo ? ' mp-bajo' : ''}`} onClick={() => setEditando(p)}>
                <td>
                  <button type="button" className="mp-nombre-boton" onClick={() => setEditando(p)}>
                    {p.nombre}
                  </button>
                  <small className="gan-categoria">
                    {p.categoria} · <Etiqueta metodo={p.metodo} />
                  </small>
                  <small className="gan-categoria mp-solo-movil">Precio {soles(p.precio)}</small>
                </td>
                <td className="gan-num mp-opcional">{soles(p.precio)}</td>
                {cobraIgv && <td className="gan-num mp-opcional">{soles(p.precio_sin_igv)}</td>}
                <td className="gan-num">{soles(p.costo)}</td>
                <td className={`gan-num gan-fuerte mp-opcional${p.ganancia < 0 ? ' gan-perdida' : ''}`}>{soles(p.ganancia)}</td>
                <td className={`gan-num${p.bajo_objetivo ? ' mp-rojo' : ''}`}>
                  {pct(p.margen)}
                  {p.objetivo !== carta.config.margen_objetivo && <small className="gan-categoria">quieres {pct(p.objetivo)}</small>}
                  {p.precio_sugerido && <small className="gan-categoria mp-solo-movil">cobra {soles(p.precio_sugerido)}</small>}
                </td>
                <td className="gan-num mp-opcional">{p.precio_sugerido ? soles(p.precio_sugerido) : '—'}</td>
              </tr>
            ))}
            {platos.length === 0 && (
              <tr>
                <td colSpan={7} className="rep-sin-resultados">
                  {filtro === 'BAJOS' ? 'Todos tus platos llegan a tu margen.' : 'No hay platos con ese filtro.'}
                </td>
              </tr>
            )}
          </tbody>
        </table>
      </div>
      <p className="gan-pie">
        “Te cuesta” es sin el IGV que recuperas si compras con factura. Precio sugerido: lo que tendrías que cobrar para ganar tu margen,
        redondeado a 50 céntimos.
      </p>
      {editando && (
        <EditorPlato
          plato={editando}
          config={config}
          onCerrar={() => setEditando(null)}
          onGuardado={() => {
            setEditando(null);
            onCambio();
          }}
        />
      )}
    </div>
  );
}

// ------------------------------------------------------------------ Un plato

function EditorPlato({ plato, config, onCerrar, onGuardado }) {
  const [detalle, setDetalle] = useState(null);
  const [insumos, setInsumos] = useState([]);
  const [modo, setModo] = useState('RECETA');
  const [lineas, setLineas] = useState([]);
  const [rinde, setRinde] = useState('1');
  const [costoFijo, setCostoFijo] = useState('');
  const [nota, setNota] = useState('');
  const [error, setError] = useState(null);
  const [guardando, setGuardando] = useState(false);
  const [nuevoInsumo, setNuevoInsumo] = useState(null);

  useEffect(() => {
    Promise.all([api.costeoPlato(plato.producto_id), api.costeoInsumos()])
      .then(([d, i]) => {
        setDetalle(d);
        setInsumos(i);
        setLineas(d.lineas.map((l) => ({ insumo_id: l.insumo_id, cantidad: String(l.cantidad) })));
        setRinde(String(d.rinde || 1));
        setCostoFijo(d.costo_fijo ? String(d.costo_fijo) : '');
        setNota(d.nota || '');
        setModo(d.lineas.length ? 'RECETA' : d.costo_fijo ? 'FIJO' : 'APROX');
      })
      .catch((e) => setError(e.message));
  }, [plato.producto_id]);

  useEffect(() => {
    const tecla = (e) => e.key === 'Escape' && onCerrar();
    window.addEventListener('keydown', tecla);
    return () => window.removeEventListener('keydown', tecla);
  }, [onCerrar]);

  const porId = useMemo(() => new Map(insumos.map((i) => [i.id, i])), [insumos]);
  const lineasValidas = lineas.filter((l) => porId.has(Number(l.insumo_id)) && Number(l.cantidad) > 0);
  const costoReceta = lineasValidas.reduce((suma, l) => {
    const i = porId.get(Number(l.insumo_id));
    return suma + costoEfectivo((i.precio * Number(l.cantidad)) / factor(i.unidad), config, i.con_igv);
  }, 0);
  const porciones = Number(rinde) > 0 ? Number(rinde) : 1;

  // Costo que quedaría con lo que está en pantalla.
  let costo = null;
  if (modo === 'RECETA' && lineasValidas.length) costo = costoReceta / porciones;
  else if (modo === 'FIJO' && Number(costoFijo) > 0) costo = costoEfectivo(Number(costoFijo), config);
  const precioSin = detalle?.precio_sin_igv ?? plato.precio_sin_igv;
  const margen = costo != null && precioSin > 0 ? ((precioSin - costo) / precioSin) * 100 : null;
  const objetivo = plato.objetivo ?? config.margen_objetivo;
  const bajo = margen != null && margen < objetivo - 0.05;

  const guardar = async () => {
    setError(null);
    const datos = { rinde: 1, lineas: [], costo_fijo: null, nota: nota.trim() || null };
    if (modo === 'RECETA') {
      if (lineas.some((l) => !l.insumo_id || !(Number(l.cantidad) > 0))) {
        setError('Elige el insumo y escribe la cantidad de cada línea (o quítala).');
        return;
      }
      datos.lineas = lineas.map((l) => ({ insumo_id: Number(l.insumo_id), cantidad: Number(l.cantidad) }));
      datos.rinde = porciones;
    } else if (modo === 'FIJO') {
      if (!(Number(costoFijo) > 0)) {
        setError('Escribe cuánto te cuesta hacer el plato.');
        return;
      }
      datos.costo_fijo = Number(costoFijo);
    }
    setGuardando(true);
    try {
      await api.costeoGuardarPlato(plato.producto_id, datos);
      onGuardado();
    } catch (e) {
      setError(e.message);
      setGuardando(false);
    }
  };

  const crearInsumo = async (datos) => {
    // Corregir el precio o el nombre de un insumo que ya está en la receta.
    if (nuevoInsumo?.id) {
      const editado = await api.costeoActualizarInsumo(nuevoInsumo.id, datos);
      setInsumos((lista) => lista.map((x) => (x.id === editado.id ? editado : x)).sort((a, b) => a.nombre.localeCompare(b.nombre)));
      setNuevoInsumo(null);
      return;
    }
    const creado = await api.costeoCrearInsumo(datos);
    setInsumos((lista) => [...lista, creado].sort((a, b) => a.nombre.localeCompare(b.nombre)));
    setLineas((ls) => {
      const vacia = ls.findIndex((l) => !l.insumo_id);
      if (vacia >= 0) return ls.map((l, i) => (i === vacia ? { ...l, insumo_id: creado.id } : l));
      return [...ls, { insumo_id: creado.id, cantidad: '' }];
    });
    setNuevoInsumo(null);
  };

  return (
    <div className="mp-fondo" onClick={onCerrar}>
      <div className="mp-modal" role="dialog" aria-modal="true" aria-label={`Costo de ${plato.nombre}`} onClick={(e) => e.stopPropagation()}>
        <div className="mp-modal-cabecera">
          <div>
            <h3>{plato.nombre}</h3>
            <span>
              Precio {soles(plato.precio)}
              {config.regimen !== 'RUS' && plato.precio !== plato.precio_sin_igv ? ` · sin IGV ${soles(precioSin)}` : ''}
            </span>
          </div>
          <button type="button" className="mp-cerrar" onClick={onCerrar} aria-label="Cerrar">
            ×
          </button>
        </div>

        {!detalle ? (
          error ? <p className="rep-mensaje">{error}</p> : <p className="rep-cargando">Cargando...</p>
        ) : (
          <>
            <div className="mp-modos" role="radiogroup" aria-label="Cómo sacar el costo">
              {[
                ['RECETA', 'Por receta', 'Lo que lleva, en gramos o unidades'],
                ['FIJO', 'Costo fijo', 'Ya sé cuánto me cuesta'],
                ['APROX', 'Aproximado', `Un % del precio${detalle.metodo === 'COMPRA' ? ' o su precio de compra' : ''}`],
              ].map(([valor, titulo, sub]) => (
                <button
                  key={valor}
                  type="button"
                  role="radio"
                  aria-checked={modo === valor}
                  className={modo === valor ? 'activo' : ''}
                  onClick={() => setModo(valor)}
                >
                  <strong>{titulo}</strong>
                  <span>{sub}</span>
                </button>
              ))}
            </div>

            {modo === 'RECETA' && (
              <div className="mp-receta">
                {lineas.map((l, indice) => {
                  const i = porId.get(Number(l.insumo_id));
                  const pagado = i && Number(l.cantidad) > 0 ? (i.precio * Number(l.cantidad)) / factor(i.unidad) : null;
                  return (
                    <div key={indice} className="mp-linea">
                      <select
                        value={l.insumo_id || ''}
                        onChange={(e) => setLineas((ls) => ls.map((x, j) => (j === indice ? { ...x, insumo_id: Number(e.target.value) || '' } : x)))}
                        aria-label="Insumo"
                      >
                        <option value="">Elige un insumo…</option>
                        {insumos.map((x) => (
                          <option key={x.id} value={x.id}>
                            {x.nombre} — {soles(x.precio)} / {UNIDADES[x.unidad].compra}
                          </option>
                        ))}
                      </select>
                      <label className="mp-cantidad">
                        <input
                          type="number"
                          inputMode="decimal"
                          min="0"
                          step="any"
                          value={l.cantidad}
                          onChange={(e) => setLineas((ls) => ls.map((x, j) => (j === indice ? { ...x, cantidad: e.target.value } : x)))}
                          aria-label="Cantidad"
                          placeholder="0"
                        />
                        <span>{i ? UNIDADES[i.unidad].uso : ''}</span>
                      </label>
                      <span className="mp-linea-costo">{pagado == null ? '' : soles(pagado)}</span>
                      <button
                        type="button"
                        className="mp-quitar"
                        onClick={() => i && setNuevoInsumo({ id: i.id, nombre: i.nombre, unidad: i.unidad, precio: i.precio, con_igv: i.con_igv })}
                        disabled={!i}
                        aria-label={i ? `Corregir precio de ${i.nombre}` : 'Corregir precio'}
                        title="Corregir el precio de este insumo"
                      >
                        ✎
                      </button>
                      <button type="button" className="mp-quitar" onClick={() => setLineas((ls) => ls.filter((_, j) => j !== indice))} aria-label="Quitar">
                        ×
                      </button>
                    </div>
                  );
                })}
                <div className="mp-receta-acciones">
                  <button type="button" onClick={() => setLineas((ls) => [...ls, { insumo_id: '', cantidad: '' }])}>
                    + Agregar insumo
                  </button>
                  <button type="button" onClick={() => setNuevoInsumo({ nombre: '', unidad: 'KG', precio: '', con_igv: true })}>
                    + Crear insumo nuevo
                  </button>
                </div>
                {nuevoInsumo?.id && (
                  <p className="mp-ayuda">
                    Corrigiendo <strong>{nuevoInsumo.nombre}</strong>: el nuevo precio cambia el costo de todos los platos que lo usan.
                  </p>
                )}
                {nuevoInsumo && (
                  <FormularioInsumo
                    key={nuevoInsumo.id || 'nuevo'}
                    inicial={nuevoInsumo}
                    config={config}
                    onGuardar={crearInsumo}
                    onCancelar={() => setNuevoInsumo(null)}
                    compacto
                  />
                )}
                <label className="mp-rinde">
                  Esta receta rinde
                  <input type="number" inputMode="decimal" min="0" step="any" value={rinde} onChange={(e) => setRinde(e.target.value)} />
                  {porciones === 1 ? 'porción' : 'porciones'}
                </label>
                {lineasValidas.length > 0 && (
                  <p className="mp-ayuda">
                    Toda la receta: {soles(costoReceta)}
                    {porciones !== 1 ? ` ÷ ${cant(porciones)} = ${soles(costoReceta / porciones)} por porción` : ''}
                    {recupera(config) ? ' (sin el IGV que recuperas)' : ''}.
                  </p>
                )}
              </div>
            )}

            {modo === 'FIJO' && (
              <label className="mp-campo">
                ¿Cuánto te cuesta hacer un plato? (lo que pagas por sus ingredientes)
                <div className="mp-soles">
                  <span>S/</span>
                  <input type="number" inputMode="decimal" min="0" step="any" value={costoFijo} onChange={(e) => setCostoFijo(e.target.value)} autoFocus />
                </div>
                {recupera(config) && <small>Como compras con factura, se toma sin el 18 % de IGV: {soles(costoEfectivo(Number(costoFijo) || 0, config))}.</small>}
              </label>
            )}

            {modo === 'APROX' && (
              <p className="mp-ayuda">
                {detalle.metodo === 'COMPRA'
                  ? `Se usa su precio de compra: ${soles(detalle.costo)}.`
                  : `Se calcula con el ${detalle.metodo === 'CATEGORIA' ? '% de su categoría' : `${pct(config.costo_porcentaje)} general`} (en Ajustes). Es aproximado: con la receta o el costo fijo el margen sale exacto.`}
              </p>
            )}

            {modo !== 'APROX' && (
              <div className={`mp-resultado${bajo ? ' bajo' : ''}`}>
                <div>
                  <span>Te cuesta</span>
                  <strong>{costo == null ? '—' : soles(costo)}</strong>
                </div>
                <div>
                  <span>Ganas</span>
                  <strong>{costo == null ? '—' : soles(precioSin - costo)}</strong>
                </div>
                <div>
                  <span>Margen</span>
                  <strong>{pct(margen == null ? null : Math.round(margen * 10) / 10)}</strong>
                </div>
              </div>
            )}
            {bajo && (
              <p className="mp-rojo mp-ayuda">
                Debajo del {pct(objetivo)} que quieres. Para llegar, el precio tendría que ser{' '}
                {soles(sugerido(costo, objetivo, precioSin, plato.precio))}.
              </p>
            )}

            <label className="mp-campo">
              Nota (opcional)
              <input type="text" maxLength={300} value={nota} onChange={(e) => setNota(e.target.value)} placeholder="Ej.: precios del mercado de octubre" />
            </label>

            {error && (
              <p className="rep-mensaje" role="alert">
                {error}
              </p>
            )}
            <div className="mp-modal-pie">
              <button type="button" className="mp-secundario" onClick={onCerrar}>
                Cancelar
              </button>
              <button type="button" className="mp-primario" onClick={guardar} disabled={guardando}>
                {guardando ? 'Guardando…' : 'Guardar'}
              </button>
            </div>
          </>
        )}
      </div>
    </div>
  );
}

/** Precio de carta para llegar al margen objetivo (igual que el servidor). */
function sugerido(costo, objetivo, precioSin, precio) {
  const sin = costo / (1 - objetivo / 100);
  const conIgv = precioSin > 0 ? sin * (precio / precioSin) : sin;
  return Math.ceil((conIgv - 1e-9) * 2) / 2;
}

// ------------------------------------------------------------------ Insumos

function FormularioInsumo({ inicial, config, onGuardar, onCancelar, compacto = false }) {
  const [datos, setDatos] = useState({ ...inicial, precio: inicial.precio === '' ? '' : String(inicial.precio) });
  const [error, setError] = useState(null);
  const [guardando, setGuardando] = useState(false);
  const enviar = async (e) => {
    e.preventDefault();
    if (!datos.nombre.trim()) return setError('Escribe el nombre.');
    if (!(Number(datos.precio) > 0)) return setError('Escribe el precio que pagas.');
    setGuardando(true);
    setError(null);
    try {
      await onGuardar({ nombre: datos.nombre.trim(), unidad: datos.unidad, precio: Number(datos.precio), con_igv: datos.con_igv });
    } catch (err) {
      setError(err.message);
      setGuardando(false);
    }
  };
  return (
    <form className={`mp-insumo-form${compacto ? ' compacto' : ''}`} onSubmit={enviar}>
      <input
        type="text"
        placeholder="Insumo (ej.: Arroz)"
        value={datos.nombre}
        maxLength={80}
        onChange={(e) => setDatos({ ...datos, nombre: e.target.value })}
        aria-label="Nombre del insumo"
        autoFocus
      />
      <div className="mp-soles">
        <span>S/</span>
        <input
          type="number"
          inputMode="decimal"
          min="0"
          step="any"
          placeholder="0.00"
          value={datos.precio}
          onChange={(e) => setDatos({ ...datos, precio: e.target.value })}
          aria-label="Precio"
        />
      </div>
      <select value={datos.unidad} onChange={(e) => setDatos({ ...datos, unidad: e.target.value })} aria-label="Por">
        <option value="KG">por kilo</option>
        <option value="LITRO">por litro</option>
        <option value="UNIDAD">por unidad</option>
      </select>
      {recupera(config) && (
        <label className="mp-check">
          <input type="checkbox" checked={datos.con_igv} onChange={(e) => setDatos({ ...datos, con_igv: e.target.checked })} />
          Su factura trae IGV
        </label>
      )}
      <div className="mp-insumo-botones">
        <button type="button" className="mp-secundario" onClick={onCancelar}>
          Cancelar
        </button>
        <button type="submit" className="mp-primario" disabled={guardando}>
          {guardando ? 'Guardando…' : 'Guardar'}
        </button>
      </div>
      {error && <p className="mp-rojo mp-error-form">{error}</p>}
    </form>
  );
}

function VistaInsumos({ config, onCambio }) {
  const [insumos, setInsumos] = useState(null);
  const [error, setError] = useState(null);
  const [editando, setEditando] = useState(null);
  const [buscar, setBuscar] = useState('');

  const cargar = () =>
    api
      .costeoInsumos()
      .then(setInsumos)
      .catch((e) => setError(e.message));
  useEffect(() => {
    cargar();
  }, []);

  const guardar = async (datos) => {
    if (editando.id) await api.costeoActualizarInsumo(editando.id, datos);
    else await api.costeoCrearInsumo(datos);
    setEditando(null);
    await cargar();
    onCambio();
  };
  const borrar = async (i) => {
    const ok = await confirmar({ titulo: `¿Quitar "${i.nombre}"?`, mensaje: 'Deja de aparecer en la lista de insumos.', textoConfirmar: 'Quitar', icono: 'eliminar' });
    if (!ok) return;
    try {
      await api.costeoBorrarInsumo(i.id);
      await cargar();
    } catch (e) {
      setError(e.message);
    }
  };

  if (!insumos) return error ? <p className="rep-mensaje">{error}</p> : <p className="rep-cargando">Cargando...</p>;
  const texto = buscar.trim().toLowerCase();
  const visibles = insumos.filter((i) => !texto || i.nombre.toLowerCase().includes(texto));

  return (
    <div>
      <p className="mp-ayuda">
        Lo que compras para cocinar, con el precio que pagas por kilo, litro o unidad. No son productos: no salen en el punto de venta ni
        mueven stock. Si cambias un precio, cambia el costo de todos los platos que lo usan.
      </p>
      <div className="mp-filtros">
        <input type="search" placeholder="Buscar insumo" value={buscar} onChange={(e) => setBuscar(e.target.value)} aria-label="Buscar insumo" />
        <button type="button" className="mp-primario" onClick={() => setEditando({ nombre: '', unidad: 'KG', precio: '', con_igv: true })}>
          + Nuevo insumo
        </button>
      </div>
      {error && <p className="rep-mensaje">{error}</p>}
      {editando && !editando.id && <FormularioInsumo inicial={editando} config={config} onGuardar={guardar} onCancelar={() => setEditando(null)} />}
      <div className="rep-tabla-wrapper">
        <table className="rep-tabla mp-tabla mp-tabla-insumos">
          <thead>
            <tr>
              <th>Insumo</th>
              <th className="gan-num">Precio</th>
              {recupera(config) && <th>IGV</th>}
              <th className="gan-num">En platos</th>
              <th />
            </tr>
          </thead>
          <tbody>
            {visibles.map((i) =>
              editando?.id === i.id ? (
                <tr key={i.id}>
                  <td colSpan={recupera(config) ? 5 : 4}>
                    <FormularioInsumo inicial={editando} config={config} onGuardar={guardar} onCancelar={() => setEditando(null)} />
                  </td>
                </tr>
              ) : (
                <tr key={i.id}>
                  <td>
                    <strong>{i.nombre}</strong>
                  </td>
                  <td className="gan-num">
                    {soles(i.precio)} / {UNIDADES[i.unidad].compra}
                  </td>
                  {recupera(config) && <td>{i.con_igv ? 'Con IGV' : 'Sin IGV'}</td>}
                  <td className="gan-num">{i.platos}</td>
                  <td className="mp-acciones">
                    <button type="button" onClick={() => setEditando(i)}>
                      Editar
                    </button>
                    <button type="button" onClick={() => borrar(i)} disabled={i.platos > 0} title={i.platos > 0 ? 'Está en una receta' : undefined}>
                      Quitar
                    </button>
                  </td>
                </tr>
              )
            )}
            {visibles.length === 0 && (
              <tr>
                <td colSpan={5} className="rep-sin-resultados">
                  {insumos.length ? 'Ningún insumo con ese nombre.' : 'Aún no tienes insumos. Crea el primero (arroz, aceite, pollo…).'}
                </td>
              </tr>
            )}
          </tbody>
        </table>
      </div>
    </div>
  );
}

// ------------------------------------------------------------------ Ajustes

function VistaAjustes({ config, onGuardado }) {
  const [d, setD] = useState({
    regimen: config.regimen,
    compras_con_factura: config.compras_con_factura,
    costo_porcentaje: String(config.costo_porcentaje),
    margen_objetivo: String(config.margen_objetivo),
    gastos_mensuales: config.gastos_mensuales ? String(config.gastos_mensuales) : '',
    categorias: config.categorias.map((c) => ({
      ...c,
      costo_porcentaje: c.costo_porcentaje == null ? '' : String(c.costo_porcentaje),
      margen_objetivo: c.margen_objetivo == null ? '' : String(c.margen_objetivo),
    })),
  });
  const [error, setError] = useState(null);
  const [guardando, setGuardando] = useState(false);
  const rus = d.regimen === 'RUS';

  const guardar = async (e) => {
    e.preventDefault();
    setError(null);
    setGuardando(true);
    try {
      const nueva = await api.costeoGuardarConfig({
        regimen: d.regimen,
        compras_con_factura: !rus && d.compras_con_factura,
        costo_porcentaje: Number(d.costo_porcentaje),
        margen_objetivo: Number(d.margen_objetivo),
        gastos_mensuales: Number(d.gastos_mensuales) || 0,
        categorias: d.categorias.map((c) => ({
          categoria_id: c.categoria_id,
          costo_porcentaje: c.costo_porcentaje === '' ? null : Number(c.costo_porcentaje),
          margen_objetivo: c.margen_objetivo === '' ? null : Number(c.margen_objetivo),
        })),
      });
      onGuardado(nueva);
    } catch (err) {
      setError(err.message);
      setGuardando(false);
    }
  };

  return (
    <form className="mp-ajustes" onSubmit={guardar}>
      <fieldset>
        <legend>¿Cobras IGV?</legend>
        {Object.entries(REGIMENES).map(([valor, texto]) => (
          <label key={valor} className="mp-opcion">
            <input type="radio" name="regimen" checked={d.regimen === valor} onChange={() => setD({ ...d, regimen: valor })} />
            <span>
              <strong>{texto}</strong>
              {valor === 'GENERAL' && (
                <small>
                  Tasa de tu negocio: {pct(config.tasa_igv)} (18 % normal; 10.5 % restaurante MYPE). Se cambia en Configuración → IGV. Al
                  margen se le quita ese IGV, porque es de SUNAT.
                </small>
              )}
              {valor === 'RUS' && <small>No cobras IGV: todo lo que cobras es tuyo.</small>}
              {valor === 'NO_SE' && <small>Se calcula como si cobraras IGV, para no darte un margen inflado. Pregúntale a tu contador.</small>}
            </span>
          </label>
        ))}
      </fieldset>

      <fieldset disabled={rus}>
        <legend>Tus compras</legend>
        <label className="mp-opcion">
          <input type="checkbox" checked={!rus && d.compras_con_factura} onChange={(e) => setD({ ...d, compras_con_factura: e.target.checked })} />
          <span>
            <strong>Compro mis insumos con factura</strong>
            <small>
              Recuperas el IGV de lo que compras, así que el costo se toma sin ese 18 %. Si compras en el mercado sin factura, déjalo sin
              marcar.
            </small>
          </span>
        </label>
      </fieldset>

      <fieldset>
        <legend>Tus metas y aproximados</legend>
        <label className="mp-campo">
          Margen que quieres ganar en cada plato
          <div className="mp-soles">
            <input type="number" inputMode="decimal" min="0" max="99" step="any" value={d.margen_objetivo} onChange={(e) => setD({ ...d, margen_objetivo: e.target.value })} />
            <span>%</span>
          </div>
          <small>Los platos que ganen menos salen en rojo. En restaurantes suele estar entre 60 % y 70 % (food cost de 30 % a 40 %).</small>
        </label>
        <label className="mp-campo">
          Costo aproximado de un plato sin receta
          <div className="mp-soles">
            <input type="number" inputMode="decimal" min="1" max="99" step="any" value={d.costo_porcentaje} onChange={(e) => setD({ ...d, costo_porcentaje: e.target.value })} />
            <span>% de su precio</span>
          </div>
          <small>Solo mientras no le pongas su receta o su costo. Ese plato se marca “aprox.”.</small>
        </label>
        <label className="mp-campo">
          Gastos fijos aproximados del mes (si no los anotas en Gastos)
          <div className="mp-soles">
            <span>S/</span>
            <input type="number" inputMode="decimal" min="0" step="any" value={d.gastos_mensuales} onChange={(e) => setD({ ...d, gastos_mensuales: e.target.value })} placeholder="0.00" />
          </div>
          <small>Alquiler, luz, gas, sueldos. Si el mes tiene gastos registrados en la pantalla Gastos, se usan esos.</small>
        </label>
      </fieldset>

      {d.categorias.length > 0 && (
        <fieldset>
          <legend>Por categoría (opcional)</legend>
          <p className="mp-ayuda">
            Las bebidas que compras y revendes ganan menos que un plato: ponles su propio margen para que no salgan en rojo. Vacío = usa el
            general.
          </p>
          <div className="mp-categorias">
            <div className="mp-categoria mp-categoria-titulos" aria-hidden="true">
              <span />
              <small>Margen que quieres</small>
              <small>Costo aprox.</small>
            </div>
            {d.categorias.map((c, i) => {
              const cambiar = (campo) => (e) =>
                setD({ ...d, categorias: d.categorias.map((x, j) => (j === i ? { ...x, [campo]: e.target.value } : x)) });
              return (
                <div key={c.categoria_id} className="mp-categoria">
                  <span>{c.nombre}</span>
                  <div className="mp-soles">
                    <input
                      type="number"
                      inputMode="decimal"
                      min="0"
                      max="99"
                      step="any"
                      value={c.margen_objetivo}
                      placeholder={String(d.margen_objetivo || '')}
                      onChange={cambiar('margen_objetivo')}
                      aria-label={`Margen que quieres en ${c.nombre}`}
                    />
                    <span>%</span>
                  </div>
                  <div className="mp-soles">
                    <input
                      type="number"
                      inputMode="decimal"
                      min="1"
                      max="99"
                      step="any"
                      value={c.costo_porcentaje}
                      placeholder={String(d.costo_porcentaje || '')}
                      onChange={cambiar('costo_porcentaje')}
                      aria-label={`Costo aproximado en ${c.nombre}`}
                    />
                    <span>%</span>
                  </div>
                </div>
              );
            })}
          </div>
        </fieldset>
      )}

      {error && (
        <p className="rep-mensaje" role="alert">
          {error}
        </p>
      )}
      <div className="mp-modal-pie">
        <button type="submit" className="mp-primario" disabled={guardando}>
          {guardando ? 'Guardando…' : 'Guardar ajustes'}
        </button>
      </div>
    </form>
  );
}

// ------------------------------------------------------------------ Excel

/** Libro con el resumen del mes, los platos vendidos, la carta y los insumos. */
function libroExcel(reporte, carta, insumos) {
  const s = reporte.resumen;
  const cfg = reporte.config;
  const cobra = cfg.regimen !== 'RUS';
  const fuente = { REGISTRADOS: 'registrados en Gastos', ESTIMADOS: 'aproximados (Ajustes)', NINGUNO: 'no hay' }[s.gastos_fuente] || '';
  const resumen = [
    ['Concepto', 'Monto (S/)', 'Detalle'],
    ['Mes', mayuscula(nombreMes(reporte.mes)), ''],
    ['Régimen', REGIMENES[cfg.regimen] || cfg.regimen, cobra ? `IGV ${reporte.tasa_igv} %` : 'sin IGV'],
    ['Compras con factura', cobra && cfg.compras_con_factura ? 'Sí' : 'No', cobra && cfg.compras_con_factura ? 'El costo va sin el 18 % de IGV' : ''],
    ['Lo que cobraste (platos y envíos)', s.ventas, `Ya restadas devoluciones: ${numero(s.devoluciones)}`],
    ['IGV (de SUNAT)', s.igv, ''],
    ['Ventas sin IGV', s.ventas_sin_igv, ''],
    ['  de platos', s.platos_sin_igv, ''],
    ['  de envíos (delivery)', s.envios_sin_igv, 'Sin costo de insumos'],
    ['Costo de los platos', s.costo, s.costo_estimado ? `${s.costo_estimado} % es aproximado` : 'Todo exacto'],
    ['Food cost (%)', s.food_cost ?? '', 'Costo ÷ venta de platos sin IGV'],
    ['Ganancia de los platos', s.ganancia_platos, `Margen ${s.margen_platos ?? '—'} % (objetivo ${cfg.margen_objetivo} %)`],
    ['Ganancia bruta', s.ganancia_bruta, 'Platos + envíos'],
    ['Gastos del mes', s.gastos, fuente],
    ['Ganancia neta', s.ganancia_neta, `Margen neto ${s.margen_neto ?? '—'} %`],
    ['Punto de equilibrio', s.punto_equilibrio ?? '', 'Lo que hay que cobrar en el mes para cubrir los gastos'],
    ['Platos bajo el objetivo', s.platos_bajo_objetivo, ''],
  ];
  const platos = [
    ['Plato', 'Categoría', 'Cantidad', 'Precio promedio', 'Vendido', 'Vendido sin IGV', 'Costo c/u', 'Costo', 'Ganancia', 'Margen %', 'Objetivo %', 'Aporte %', 'Costo según', 'Bajo objetivo'],
    ...reporte.platos.map((p) => [
      p.nombre,
      p.categoria,
      p.cantidad,
      p.precio_promedio ?? '',
      p.venta,
      p.venta_sin_igv,
      p.costo_unitario ?? '',
      p.costo,
      p.ganancia,
      p.margen ?? '',
      p.objetivo,
      p.participacion ?? '',
      (METODOS[p.metodo] || METODOS.GENERAL).texto + (METODOS[p.metodo]?.exacto ? '' : ' (aprox.)'),
      p.bajo_objetivo ? 'SÍ' : '',
    ]),
  ];
  const totalPlatos = reporte.platos.reduce(
    (t, p) => ({ venta: t.venta + p.venta, sin: t.sin + p.venta_sin_igv, costo: t.costo + p.costo, ganancia: t.ganancia + p.ganancia }),
    { venta: 0, sin: 0, costo: 0, ganancia: 0 }
  );
  platos.push(['TOTAL', '', '', '', r2(totalPlatos.venta), r2(totalPlatos.sin), '', r2(totalPlatos.costo), r2(totalPlatos.ganancia), s.margen_platos ?? '', '', '', '', '']);
  const cartaFilas = [
    ['Plato', 'Categoría', 'Precio', 'Precio sin IGV', 'Te cuesta', 'Ganas', 'Margen %', 'Objetivo %', 'Costo según', 'Bajo objetivo', 'Precio sugerido'],
    ...carta.platos.map((p) => [
      p.nombre,
      p.categoria,
      p.precio,
      p.precio_sin_igv,
      p.costo,
      p.ganancia,
      p.margen ?? '',
      p.objetivo,
      (METODOS[p.metodo] || METODOS.GENERAL).texto + (METODOS[p.metodo]?.exacto ? '' : ' (aprox.)'),
      p.bajo_objetivo ? 'SÍ' : '',
      p.precio_sugerido ?? '',
    ]),
  ];
  const insumosFilas = [
    ['Insumo', 'Precio (S/)', 'Por', 'Factura con IGV', 'En platos'],
    ...insumos.map((i) => [i.nombre, i.precio, UNIDADES[i.unidad].compra, i.con_igv ? 'Sí' : 'No', i.platos]),
  ];
  return escribirLibro([
    { nombre: 'Resumen', filas: resumen, anchos: [34, 16, 48], soles: [1] },
    { nombre: 'Platos del mes', filas: platos, anchos: [30, 18, 10, 14, 13, 15, 12, 12, 12, 10, 10, 10, 24, 13], soles: [3, 4, 5, 6, 7, 8], negritas: [platos.length - 1] },
    { nombre: 'Carta', filas: cartaFilas, anchos: [30, 18, 11, 14, 12, 12, 10, 10, 24, 13, 15], soles: [2, 3, 4, 5, 10] },
    { nombre: 'Insumos', filas: insumosFilas, anchos: [30, 12, 10, 16, 10], soles: [1] },
  ]);
}
