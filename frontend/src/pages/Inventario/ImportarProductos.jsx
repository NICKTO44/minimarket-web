import { useMemo, useState } from 'react';
import { api } from '../../api/api';
import { ErrorDeArchivo, TIPO_XLSX, escribirXlsx, leerHoja } from '../../utils/hojas';
import { CAMPOS, armarFilas, columnasDe, filaDeTitulos, sugerirCampos } from '../../utils/importacion';
import { descargarArchivo } from '../../utils/pdfTicket';
import { opcionesUnidad } from '../../utils/unidades';
import './ImportarProductos.css';

// Importar productos desde el Excel o CSV de otro sistema (solo el
// administrador). Tres pasos: subir el archivo, decir qué es cada columna y
// revisar antes de guardar. El servidor es quien decide qué fila está lista,
// cuál ya existe y cuál tiene un error; nada se guarda hasta confirmar.

const MAXIMO_FILAS = 5000;
const FILAS_A_LA_VISTA = 200;
const TITULOS_PLANTILLA = ['Código', 'Nombre', 'Descripción', 'Categoría', 'Unidad', 'Precio de venta', 'Precio de compra', 'Stock', 'Stock mínimo'];
const OBLIGATORIOS = CAMPOS.filter((c) => c.obligatorio);

const soles = (n) => (n == null ? '—' : `S/ ${n.toFixed(2)}`);
const plural = (n, uno, varios) => `${n.toLocaleString('es-PE')} ${n === 1 ? uno : varios}`;

export default function ImportarProductos({ categorias, unidadesActivas, onCerrar, onTerminado }) {
  const [paso, setPaso] = useState('ARCHIVO');
  const [ocupado, setOcupado] = useState(false);
  const [error, setError] = useState(null);
  const [arrastrando, setArrastrando] = useState(false);

  const [nombreArchivo, setNombreArchivo] = useState('');
  const [hoja, setHoja] = useState([]);
  // Fila de títulos que se detectó (-1 = el archivo empieza directo con productos).
  const [detectada, setDetectada] = useState(-1);
  const [conTitulos, setConTitulos] = useState(true);
  // { indiceDeColumna: campo }
  const [mapa, setMapa] = useState({});
  // Columnas que el sistema reconoció solo (para decirlo en pantalla).
  const [reconocidas, setReconocidas] = useState({});
  const [categoriaDefecto, setCategoriaDefecto] = useState(() => categorias.find((c) => c.nombre === 'General')?.nombre || 'General');
  const [unidadDefecto, setUnidadDefecto] = useState('UNIDAD');

  const [armado, setArmado] = useState({ filas: [], unidadesRaras: 0 });
  const [revision, setRevision] = useState(null);
  const [existentes, setExistentes] = useState('SALTAR');
  const [soloProblemas, setSoloProblemas] = useState(false);
  const [resultado, setResultado] = useState(null);

  const filaTitulos = conTitulos ? (detectada >= 0 ? detectada : hoja.findIndex((f) => f.some((c) => c !== ''))) : -1;
  const columnas = useMemo(() => columnasDe(hoja, filaTitulos), [hoja, filaTitulos]);
  const filasConDatos = useMemo(
    () => hoja.filter((f, i) => i > filaTitulos && f.some((c) => String(c).trim() !== '')).length,
    [hoja, filaTitulos]
  );
  const usados = new Set(Object.values(mapa).filter(Boolean));
  const faltan = OBLIGATORIOS.filter((c) => !usados.has(c.valor));
  const opcionesCategoria = categorias.some((c) => c.nombre === 'General') ? categorias : [...categorias, { id: 'general', nombre: 'General' }];

  const abrirArchivo = async (archivo) => {
    if (!archivo || ocupado) return;
    setOcupado(true);
    setError(null);
    try {
      const filas = await leerHoja(archivo);
      const titulos = filaDeTitulos(filas);
      const primera = titulos >= 0 ? titulos : filas.findIndex((f) => f.some((c) => c !== ''));
      if (primera < 0) throw new ErrorDeArchivo('El archivo no tiene datos.');
      const cols = columnasDe(filas, titulos);
      const sugeridos = titulos >= 0 ? sugerirCampos(cols.map((c) => c.titulo)) : cols.map(() => '');
      const nuevoMapa = {};
      const auto = {};
      cols.forEach((c, i) => {
        nuevoMapa[c.indice] = sugeridos[i];
        if (sugeridos[i]) auto[c.indice] = true;
      });
      setHoja(filas);
      setNombreArchivo(archivo.name);
      setDetectada(titulos);
      setConTitulos(titulos >= 0);
      setMapa(nuevoMapa);
      setReconocidas(auto);
      setPaso('COLUMNAS');
    } catch (e) {
      setError(e instanceof ErrorDeArchivo ? e.message : 'No se pudo leer el archivo. Revisa que sea un Excel (.xlsx) o un CSV.');
    } finally {
      setOcupado(false);
    }
  };

  // Un campo solo puede venir de una columna: elegirlo en otra lo quita de la anterior.
  const elegirCampo = (indice, campo) => {
    setMapa((actual) => {
      const nuevo = { ...actual };
      if (campo) for (const k of Object.keys(nuevo)) if (nuevo[k] === campo) nuevo[k] = '';
      nuevo[indice] = campo;
      return nuevo;
    });
    setReconocidas((actual) => ({ ...actual, [indice]: false }));
  };

  const pedir = (filas, soloRevisar, queHacer) =>
    api.productosImportar({
      filas,
      solo_revisar: soloRevisar,
      existentes: queHacer,
      categoria_defecto: categoriaDefecto,
      unidad_defecto: unidadDefecto,
    });

  const revisar = async () => {
    if (ocupado || faltan.length) return;
    const nuevo = armarFilas(hoja, filaTitulos, mapa);
    if (nuevo.filas.length === 0) return setError('El archivo no tiene productos debajo de los títulos.');
    if (nuevo.filas.length > MAXIMO_FILAS) {
      return setError(`El archivo trae ${nuevo.filas.length.toLocaleString('es-PE')} productos y el máximo por importación es ${MAXIMO_FILAS.toLocaleString('es-PE')}. Divídelo en partes.`);
    }
    setOcupado(true);
    setError(null);
    try {
      setRevision(await pedir(nuevo.filas, true, 'SALTAR'));
      setArmado(nuevo);
      setSoloProblemas(false);
      setPaso('REVISION');
    } catch (e) {
      setError(e.message);
    } finally {
      setOcupado(false);
    }
  };

  const aImportar = revision ? revision.listos + (existentes === 'ACTUALIZAR' ? revision.existentes : 0) : 0;

  const importar = async () => {
    if (ocupado || aImportar === 0) return;
    setOcupado(true);
    setError(null);
    try {
      const respuesta = await pedir(armado.filas, false, existentes);
      setResultado(respuesta);
      setRevision(respuesta);
      setPaso('LISTO');
    } catch (e) {
      setError(e.message);
    } finally {
      setOcupado(false);
    }
  };

  // Filas del archivo junto con lo que dijo el servidor de cada una.
  const filasRevisadas = useMemo(() => {
    if (!revision) return [];
    const porFila = new Map(revision.filas.map((f) => [f.fila, f]));
    return armado.filas.map((f) => ({ ...f, ...porFila.get(f.fila) }));
  }, [revision, armado]);
  const filasVisibles = soloProblemas ? filasRevisadas.filter((f) => f.estado !== 'LISTO') : filasRevisadas;

  const descargarPlantilla = () => {
    descargarArchivo(escribirXlsx([TITULOS_PLANTILLA], { anchos: [16, 36, 30, 18, 12, 15, 16, 10, 13] }), 'Plantilla-productos.xlsx', { tipo: TIPO_XLSX });
  };

  const descargarErrores = () => {
    const conError = filasRevisadas.filter((f) => f.estado === 'ERROR');
    const filas = [
      ['Fila', 'Código', 'Nombre', 'Precio de venta', 'Stock', 'Categoría', 'Problema'],
      ...conError.map((f) => [f.fila, f.codigo, f.nombre, f.precio ?? '', f.stock ?? '', f.categoria || '', f.motivo || '']),
    ];
    descargarArchivo(escribirXlsx(filas, { hoja: 'Con error', anchos: [7, 16, 36, 15, 10, 18, 44] }), 'Productos-con-error.xlsx', { tipo: TIPO_XLSX });
  };

  const cerrar = () => {
    if (ocupado) return;
    if (resultado?.guardado) onTerminado(resultado);
    else onCerrar();
  };

  return (
    <div className="inv-modal-overlay" onClick={paso === 'ARCHIVO' ? cerrar : undefined}>
      <div className="inv-modal imp-modal" onClick={(e) => e.stopPropagation()} role="dialog" aria-label="Importar productos">
        {paso === 'ARCHIVO' && (
          <>
            <h2>Importar productos</h2>
            <p className="imp-nota">
              Sube el Excel o CSV de tu sistema anterior. Nada se guarda hasta que revises y confirmes.
            </p>
            <label
              className={`imp-zona${arrastrando ? ' encima' : ''}`}
              onDragOver={(e) => {
                e.preventDefault();
                setArrastrando(true);
              }}
              onDragLeave={() => setArrastrando(false)}
              onDrop={(e) => {
                e.preventDefault();
                setArrastrando(false);
                abrirArchivo(e.dataTransfer.files?.[0]);
              }}
            >
              <input
                type="file"
                accept=".xlsx,.xls,.csv,.txt"
                onChange={(e) => {
                  abrirArchivo(e.target.files?.[0]);
                  e.target.value = '';
                }}
              />
              <strong>{ocupado ? 'Leyendo el archivo…' : 'Elige tu archivo o arrástralo aquí'}</strong>
              <span>Excel (.xlsx) o CSV · hasta {MAXIMO_FILAS.toLocaleString('es-PE')} productos</span>
            </label>
            {error && <p className="inv-mensaje inv-mensaje-error">{error}</p>}
            <p className="imp-nota imp-nota-pie">
              Debe traer al menos <strong>código, nombre y precio de venta</strong>. ¿No tienes archivo?{' '}
              <button type="button" className="imp-enlace" onClick={descargarPlantilla}>
                Descarga la plantilla
              </button>{' '}
              con las columnas listas para llenar.
            </p>
            <div className="inv-modal-acciones">
              <button className="inv-boton-cancelar" onClick={cerrar}>
                Cancelar
              </button>
            </div>
          </>
        )}

        {paso === 'COLUMNAS' && (
          <>
            <h2>¿Qué es cada columna?</h2>
            <p className="imp-nota">
              Archivo: <strong>{nombreArchivo}</strong> · {plural(filasConDatos, 'fila con datos', 'filas con datos')}. Revisa que cada
              columna esté bien identificada; las que queden en "No importar" se ignoran.
            </p>
            <div className="imp-tabla-caja">
              <table className="imp-tabla imp-tabla-columnas">
                <thead>
                  <tr>
                    <th>Columna en tu archivo</th>
                    <th>Ejemplo</th>
                    <th>Es…</th>
                    <th></th>
                  </tr>
                </thead>
                <tbody>
                  {columnas.map((c) => (
                    <tr key={c.indice}>
                      <td>
                        <strong>{c.titulo}</strong>
                      </td>
                      <td className="imp-ejemplo">{c.ejemplo || '—'}</td>
                      <td>
                        <select value={mapa[c.indice] || ''} onChange={(e) => elegirCampo(c.indice, e.target.value)} aria-label={`Qué es la columna ${c.titulo}`}>
                          <option value="">— No importar —</option>
                          {CAMPOS.map((campo) => (
                            <option key={campo.valor} value={campo.valor}>
                              {campo.label}
                            </option>
                          ))}
                        </select>
                      </td>
                      <td>
                        {mapa[c.indice] ? (
                          <span className="imp-ok">{reconocidas[c.indice] ? '✓ reconocida' : '✓ elegida'}</span>
                        ) : (
                          <span className="imp-apagada">No se importa</span>
                        )}
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>

            <label className="imp-casilla">
              <input type="checkbox" checked={conTitulos} onChange={(e) => setConTitulos(e.target.checked)} />
              La primera fila trae los títulos de las columnas
            </label>

            <div className="imp-defectos">
              <label>
                {usados.has('categoria') ? 'Los productos sin categoría entran a' : 'Tu archivo no trae categoría: todos entran a'}
                <select value={categoriaDefecto} onChange={(e) => setCategoriaDefecto(e.target.value)}>
                  {opcionesCategoria.map((c) => (
                    <option key={c.id} value={c.nombre}>
                      {c.nombre}
                    </option>
                  ))}
                </select>
              </label>
              <label>
                {usados.has('unidad') ? 'Sin unidad, se venden por' : 'Todos se venden por'}
                <select value={unidadDefecto} onChange={(e) => setUnidadDefecto(e.target.value)}>
                  {opcionesUnidad(unidadesActivas, unidadDefecto).map((u) => (
                    <option key={u.valor} value={u.valor}>
                      {u.label}
                    </option>
                  ))}
                </select>
              </label>
            </div>

            {faltan.length > 0 && (
              <p className="inv-mensaje imp-mensaje-aviso">
                Falta indicar qué columna es: <strong>{faltan.map((c) => c.label).join(', ')}</strong>.
              </p>
            )}
            {error && <p className="inv-mensaje inv-mensaje-error">{error}</p>}
            <div className="inv-modal-acciones">
              <button
                className="inv-boton-cancelar"
                onClick={() => {
                  setError(null);
                  setPaso('ARCHIVO');
                }}
                disabled={ocupado}
              >
                ← Cambiar archivo
              </button>
              <button className="inv-boton-guardar" onClick={revisar} disabled={ocupado || faltan.length > 0}>
                {ocupado ? 'Revisando…' : 'Revisar productos →'}
              </button>
            </div>
          </>
        )}

        {paso === 'REVISION' && revision && (
          <>
            <h2>Revisa antes de importar</h2>
            <div className="imp-resumen">
              <div className="imp-dato imp-dato-listo">
                <strong>{revision.listos.toLocaleString('es-PE')}</strong>
                <span>listos para importar</span>
              </div>
              <div className="imp-dato imp-dato-existe">
                <strong>{revision.existentes.toLocaleString('es-PE')}</strong>
                <span>ya existen en el sistema</span>
              </div>
              <div className="imp-dato imp-dato-error">
                <strong>{revision.errores.toLocaleString('es-PE')}</strong>
                <span>con error (no se importan)</span>
              </div>
            </div>
            {(revision.categorias_nuevas.length > 0 || armado.unidadesRaras > 0) && (
              <ul className="imp-avisos">
                {revision.categorias_nuevas.length > 0 && (
                  <li>
                    Se {revision.categorias_nuevas.length === 1 ? 'creará la categoría' : 'crearán las categorías'}:{' '}
                    <strong>{revision.categorias_nuevas.join(', ')}</strong>.
                  </li>
                )}
                {armado.unidadesRaras > 0 && (
                  <li>
                    {plural(armado.unidadesRaras, 'producto trae una unidad que no se reconoció', 'productos traen una unidad que no se reconoció')}:
                    entran con la unidad que elegiste.
                  </li>
                )}
              </ul>
            )}

            <div className="imp-tabla-caja imp-tabla-revision">
              <table className="imp-tabla">
                <thead>
                  <tr>
                    <th>Fila</th>
                    <th>Código</th>
                    <th>Nombre</th>
                    <th>Categoría</th>
                    <th className="imp-num">Precio</th>
                    <th className="imp-num">Stock</th>
                    <th>Estado</th>
                  </tr>
                </thead>
                <tbody>
                  {filasVisibles.slice(0, FILAS_A_LA_VISTA).map((f) => (
                    <tr key={f.fila} className={f.estado === 'ERROR' ? 'imp-fila-error' : f.estado === 'EXISTE' ? 'imp-fila-existe' : ''}>
                      <td>{f.fila}</td>
                      <td>{f.codigo || '—'}</td>
                      <td>{f.nombre || '—'}</td>
                      <td>
                        {f.categoria}
                        {f.categoria_nueva && f.estado === 'LISTO' && <em className="imp-nueva">nueva</em>}
                      </td>
                      <td className="imp-num">{soles(f.precio)}</td>
                      <td className="imp-num">{f.stock ?? '—'}</td>
                      <td className="imp-estado">{f.estado === 'LISTO' ? 'Listo' : f.motivo}</td>
                    </tr>
                  ))}
                  {filasVisibles.length === 0 && (
                    <tr>
                      <td colSpan={7} className="imp-vacio">
                        Ninguna fila tiene problemas.
                      </td>
                    </tr>
                  )}
                </tbody>
              </table>
            </div>
            <div className="imp-bajo-tabla">
              <label className="imp-casilla">
                <input type="checkbox" checked={soloProblemas} onChange={(e) => setSoloProblemas(e.target.checked)} />
                Ver solo las filas con problema
              </label>
              {filasVisibles.length > FILAS_A_LA_VISTA && (
                <span>
                  Se muestran las primeras {FILAS_A_LA_VISTA} de {filasVisibles.length.toLocaleString('es-PE')} filas.
                </span>
              )}
            </div>

            {revision.existentes > 0 && (
              <label className="imp-existentes">
                {revision.existentes === 1 ? 'El que ya existe:' : `Los ${revision.existentes.toLocaleString('es-PE')} que ya existen:`}
                <select value={existentes} onChange={(e) => setExistentes(e.target.value)}>
                  <option value="SALTAR">No tocarlos (saltar)</option>
                  <option value="ACTUALIZAR">Actualizar su precio y stock</option>
                </select>
              </label>
            )}

            {error && <p className="inv-mensaje inv-mensaje-error">{error}</p>}
            <div className="inv-modal-acciones">
              <button
                className="inv-boton-cancelar"
                onClick={() => {
                  setError(null);
                  setPaso('COLUMNAS');
                }}
                disabled={ocupado}
              >
                ← Atrás
              </button>
              <button className="inv-boton-guardar" onClick={importar} disabled={ocupado || aImportar === 0}>
                {ocupado
                  ? 'Importando…'
                  : aImportar === 0
                    ? 'No hay nada que importar'
                    : existentes === 'ACTUALIZAR' && revision.existentes > 0
                      ? `Importar ${revision.listos.toLocaleString('es-PE')} y actualizar ${revision.existentes.toLocaleString('es-PE')}`
                      : `Importar ${plural(revision.listos, 'producto', 'productos')}`}
              </button>
            </div>
          </>
        )}

        {paso === 'LISTO' && resultado && (
          <>
            <h2>Importación terminada</h2>
            <ul className="imp-final">
              <li className="imp-ok">✓ {plural(resultado.creados, 'producto nuevo importado', 'productos nuevos importados')}.</li>
              {resultado.actualizados > 0 && <li className="imp-ok">✓ {plural(resultado.actualizados, 'producto actualizado', 'productos actualizados')}.</li>}
              {resultado.categorias_nuevas.length > 0 && <li>Categorías creadas: {resultado.categorias_nuevas.join(', ')}.</li>}
              {resultado.existentes > resultado.actualizados && (
                <li>{plural(resultado.existentes - resultado.actualizados, 'producto ya existía y no se tocó', 'productos ya existían y no se tocaron')}.</li>
              )}
              {resultado.errores > 0 && (
                <li className="imp-pendiente">
                  {plural(resultado.errores, 'fila tenía un error y no entró', 'filas tenían un error y no entraron')}.{' '}
                  <button type="button" className="imp-enlace" onClick={descargarErrores}>
                    Descargar esas filas
                  </button>{' '}
                  para corregirlas y volver a importarlas.
                </li>
              )}
            </ul>
            <div className="inv-modal-acciones">
              <button className="inv-boton-guardar" onClick={cerrar}>
                Ver mis productos
              </button>
            </div>
          </>
        )}
      </div>
    </div>
  );
}
