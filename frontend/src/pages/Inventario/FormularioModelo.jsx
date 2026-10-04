// Modelo con tallas y colores (módulo "Tallas y colores", rubro Ropa y calzado).
// Se llena una sola vez: nombre, tallas, colores y una cuadrícula con el
// precio, el stock y el código de barras de cada combinación. Por dentro,
// cada fila es un producto normal ("Polo básico · M · Negro").
import { useMemo, useRef, useState } from 'react';
import { api, API_URL } from '../../api/api';
import EscanerCodigoBarras from '../../components/EscanerCodigoBarras';
import { comprimirImagen } from '../../utils/comprimirImagen';
import { opcionesUnidad } from '../../utils/unidades';
import { TALLAS_SUGERIDAS, compararTallas, leerLista, nombreVariante } from '../../utils/variantes';
import './FormularioModelo.css';

const clave = (talla, color) => `${talla.toLowerCase()}|${color.toLowerCase()}`;
const aNumero = (texto) => parseFloat(String(texto).replace(',', '.'));

// Filas iniciales al editar: una por cada talla/color que ya existe.
function filasDe(grupo) {
  const filas = {};
  for (const v of grupo?.variantes || []) {
    filas[clave(v.talla || '', v.color || '')] = {
      id: v.id,
      incluida: true,
      precio: String(v.precio),
      stock: String(v.stock),
      stockOriginal: String(v.stock),
      codigo: v.codigo,
      nombre: v.nombre,
    };
  }
  return filas;
}

// ganancias: con el módulo "Reporte de ganancias" el precio de compra es obligatorio.
export default function FormularioModelo({
  grupo = null,
  categorias,
  unidadesActivas,
  onCerrar,
  onGuardado,
  onQuitarVariante,
  onCategorias,
  ganancias = false,
}) {
  const editando = !!grupo;
  // Categorías: se puede crear una nueva sin salir de este formulario.
  const [listaCategorias, setListaCategorias] = useState(categorias);
  const [nuevaCategoria, setNuevaCategoria] = useState(null);
  const [creandoCategoria, setCreandoCategoria] = useState(false);
  const [nombre, setNombre] = useState(grupo?.nombre || '');
  const [categoriaId, setCategoriaId] = useState(grupo ? String(grupo.categoria_id) : '');
  const [unidad, setUnidad] = useState(grupo?.unidad_medida || 'UNIDAD');
  const [precioCompra, setPrecioCompra] = useState(
    grupo?.variantes[0]?.precio_compra ? String(grupo.variantes[0].precio_compra) : ''
  );
  const [stockMinimo, setStockMinimo] = useState(grupo ? String(grupo.variantes[0]?.stock_minimo ?? 1) : '1');
  const [tallas, setTallas] = useState(grupo?.tallas || []);
  const [colores, setColores] = useState(grupo?.colores || []);
  const [textoTalla, setTextoTalla] = useState('');
  const [textoColor, setTextoColor] = useState('');
  // clave "talla|color" -> { id, incluida, precio, stock, codigo }
  const [filas, setFilas] = useState(() => filasDe(grupo));
  // Precio de cada talla: llena todas las filas de esa talla.
  const [precioTalla, setPrecioTalla] = useState({});
  const [imagenArchivo, setImagenArchivo] = useState(null);
  const [imagenPreview, setImagenPreview] = useState(() =>
    grupo?.imagen_url ? `${API_URL}${grupo.imagen_url}?t=${Date.now()}` : null
  );
  const [escaneando, setEscaneando] = useState(null);
  const [guardando, setGuardando] = useState(false);
  const [mensaje, setMensaje] = useState(null);
  const tablaRef = useRef(null);

  // Todas las combinaciones de lo elegido. Sin colores, una fila por talla;
  // sin tallas, una por color.
  const combinaciones = useMemo(() => {
    const listaTallas = tallas.length ? tallas : [''];
    const listaColores = colores.length ? colores : [''];
    const lista = [];
    for (const talla of listaTallas) {
      for (const color of listaColores) {
        if (talla || color) lista.push({ talla, color, clave: clave(talla, color) });
      }
    }
    return lista;
  }, [tallas, colores]);

  // Con tallas Y colores, nada se asigna solo: de cada talla se marcan los
  // colores en que viene (no todas las tallas vienen en todos los colores).
  // Con solo tallas, o solo colores, cada una es una fila directamente.
  const eligeColores = tallas.length > 0 && colores.length > 0;
  const filaDe = (c) =>
    filas[c.clave] || {
      id: null,
      incluida: !eligeColores,
      precio: precioTalla[c.talla.toLowerCase()] || '',
      stock: '',
      codigo: '',
    };

  const cambiarFila = (c, cambios) => {
    setFilas((prev) => ({ ...prev, [c.clave]: { ...filaDe(c), ...(prev[c.clave] || {}), ...cambios } }));
  };

  const agregarTallas = (nuevas) => {
    setTallas((prev) => {
      const existentes = new Set(prev.map((t) => t.toLowerCase()));
      const lista = [...prev];
      for (const t of nuevas) {
        const talla = t.toUpperCase().slice(0, 20);
        if (talla && !existentes.has(talla.toLowerCase())) {
          existentes.add(talla.toLowerCase());
          lista.push(talla);
        }
      }
      return lista.sort(compararTallas);
    });
  };

  const agregarColores = (nuevos) => {
    setColores((prev) => {
      const existentes = new Set(prev.map((c) => c.toLowerCase()));
      const lista = [...prev];
      for (const c of nuevos) {
        const color = (c.charAt(0).toUpperCase() + c.slice(1)).slice(0, 30);
        if (color && !existentes.has(color.toLowerCase())) {
          existentes.add(color.toLowerCase());
          lista.push(color);
        }
      }
      return lista;
    });
  };

  // Una talla o color con variantes que ya existen no se quita desde aquí:
  // primero se quita cada una (cuida su historial de ventas).
  const tieneExistentes = (campo, valor) =>
    combinaciones.some((c) => c[campo].toLowerCase() === valor.toLowerCase() && filas[c.clave]?.id);

  const quitarTalla = (talla) => setTallas((prev) => prev.filter((t) => t !== talla));
  const quitarColor = (color) => setColores((prev) => prev.filter((c) => c !== color));

  const confirmarTexto = (texto, agregar, limpiar) => {
    const lista = leerLista(texto);
    if (lista.length) agregar(lista);
    limpiar('');
  };

  const fijarPrecioTalla = (talla, valor) => {
    setPrecioTalla((prev) => ({ ...prev, [talla.toLowerCase()]: valor }));
    setFilas((prev) => {
      const siguiente = { ...prev };
      for (const c of combinaciones) {
        if (c.talla.toLowerCase() !== talla.toLowerCase()) continue;
        const actual = prev[c.clave] || { id: null, incluida: !eligeColores, stock: '', codigo: '' };
        siguiente[c.clave] = { ...actual, precio: valor };
      }
      return siguiente;
    });
  };

  // Precio que se muestra en "Precio por talla": el escrito ahí o, si todas
  // las filas de esa talla coinciden, ese.
  const precioDeTalla = (talla) => {
    const escrito = precioTalla[talla.toLowerCase()];
    if (escrito !== undefined) return escrito;
    const precios = combinaciones
      .filter((c) => c.talla === talla && filas[c.clave]?.incluida)
      .map((c) => filas[c.clave].precio);
    return precios.length && precios.every((p) => p === precios[0]) ? precios[0] : '';
  };

  // Marca o desmarca un color de una talla. Al marcarlo toma el precio de su
  // talla. Uno que ya existe no se desmarca aquí: se quita con la × de su fila.
  const alternarColor = (c) => {
    const fila = filaDe(c);
    if (fila.id) return;
    cambiarFila(c, fila.incluida ? { incluida: false } : { incluida: true, precio: fila.precio || precioDeTalla(c.talla) });
  };

  const marcarTodos = (talla) => {
    const precio = precioDeTalla(talla);
    setFilas((prev) => {
      const siguiente = { ...prev };
      for (const c of combinaciones) {
        if (c.talla !== talla) continue;
        const actual = prev[c.clave] || { id: null, precio: '', stock: '', codigo: '' };
        if (!actual.incluida) siguiente[c.clave] = { ...actual, incluida: true, precio: actual.precio || precio };
      }
      return siguiente;
    });
  };

  const crearCategoria = async () => {
    const nombreCategoria = (nuevaCategoria || '').trim();
    if (!nombreCategoria) {
      setMensaje({ tipo: 'error', texto: 'Escribe un nombre para la categoría.' });
      return;
    }
    setCreandoCategoria(true);
    setMensaje(null);
    try {
      const creada = await api.categoriaCrear({ nombre: nombreCategoria });
      const actualizadas = await api.categorias();
      setListaCategorias(actualizadas);
      onCategorias?.(actualizadas);
      setCategoriaId(String(creada.id));
      setNuevaCategoria(null);
    } catch (e) {
      setMensaje({ tipo: 'error', texto: e.message });
    } finally {
      setCreandoCategoria(false);
    }
  };

  const manejarImagen = (e) => {
    const archivo = e.target.files?.[0];
    if (!archivo) return;
    if (!archivo.type.startsWith('image/')) {
      setMensaje({ tipo: 'error', texto: 'El archivo debe ser una imagen.' });
      return;
    }
    setImagenArchivo(archivo);
    setImagenPreview(URL.createObjectURL(archivo));
  };

  // Con lector de códigos (escribe y manda Enter): pasa a la fila siguiente.
  const alEnterEnCodigo = (e) => {
    if (e.key !== 'Enter') return;
    e.preventDefault();
    const campos = Array.from(tablaRef.current?.querySelectorAll('input[data-codigo]') || []);
    const siguiente = campos[campos.indexOf(e.currentTarget) + 1];
    if (siguiente) siguiente.focus();
    else e.currentTarget.blur();
  };

  const quitarExistente = async (c) => {
    const fila = filas[c.clave];
    if (!fila?.id) return;
    const quitada = await onQuitarVariante({ id: fila.id, nombre: fila.nombre || nombreVariante(nombre, c.talla, c.color) });
    if (quitada) {
      setFilas((prev) => {
        const siguiente = { ...prev };
        delete siguiente[c.clave];
        return siguiente;
      });
    }
  };

  const incluidas = combinaciones.filter((c) => filaDe(c).incluida);

  const guardar = async () => {
    setMensaje(null);
    if (!nombre.trim()) return setMensaje({ tipo: 'error', texto: 'Escribe el nombre del modelo (por ejemplo: Polo básico).' });
    if (!categoriaId) return setMensaje({ tipo: 'error', texto: 'Elige la categoría.' });
    if (ganancias && !(aNumero(precioCompra) > 0)) {
      return setMensaje({
        tipo: 'error',
        texto: 'Falta el precio de compra: con el reporte de ganancias activo es obligatorio (lo que te cuesta, con IGV).',
      });
    }
    if (incluidas.length === 0) {
      return setMensaje({
        tipo: 'error',
        texto: eligeColores ? 'Marca en qué colores viene cada talla.' : 'Agrega al menos una talla o un color.',
      });
    }
    const sinColor = eligeColores ? tallas.find((t) => !incluidas.some((c) => c.talla === t)) : null;
    if (sinColor) return setMensaje({ tipo: 'error', texto: `Marca al menos un color para la talla ${sinColor}, o quita esa talla.` });

    const variantes = [];
    for (const c of incluidas) {
      const fila = filaDe(c);
      const etiqueta = [c.talla, c.color].filter(Boolean).join(' · ');
      const precio = aNumero(fila.precio);
      if (Number.isNaN(precio) || precio <= 0) {
        return setMensaje({ tipo: 'error', texto: `Falta el precio de ${etiqueta}.` });
      }
      const stock = fila.stock === '' ? null : aNumero(fila.stock);
      if (stock !== null && (Number.isNaN(stock) || stock < 0)) {
        return setMensaje({ tipo: 'error', texto: `El stock de ${etiqueta} no puede ser negativo.` });
      }
      if (fila.id && !fila.codigo.trim()) {
        return setMensaje({ tipo: 'error', texto: `${etiqueta} no puede quedar sin código.` });
      }
      variantes.push({
        id: fila.id || null,
        talla: c.talla,
        color: c.color,
        codigo: fila.codigo.trim(),
        precio,
        // En una talla que ya existe el stock solo se manda si se cambió
        // aquí, para no pisar una venta hecha mientras se editaba.
        stock: fila.id ? (fila.stock !== fila.stockOriginal ? stock : null) : stock ?? 0,
      });
    }

    const datos = {
      nombre: nombre.trim(),
      categoria_id: parseInt(categoriaId, 10),
      unidad_medida: unidad,
      precio_compra: precioCompra ? aNumero(precioCompra) || 0 : 0,
      stock_minimo: stockMinimo === '' ? 0 : aNumero(stockMinimo) || 0,
      variantes,
    };

    setGuardando(true);
    try {
      const guardado = editando ? await api.modeloActualizar(grupo.modelo_id, datos) : await api.modeloCrear(datos);
      let aviso = null;
      if (imagenArchivo && guardado.variantes.length) {
        try {
          const primera = guardado.variantes[0].id;
          await api.productoSubirImagen(primera, await comprimirImagen(imagenArchivo));
          await api.modeloCompartirImagen(guardado.modelo_id, primera);
        } catch (e) {
          aviso = `Modelo guardado, pero la foto no se pudo subir (${e.message}). Ábrelo de nuevo para intentarlo.`;
        }
      }
      onGuardado(
        aviso
          ? { tipo: 'error', texto: aviso }
          : {
              tipo: 'exito',
              texto: editando
                ? `"${guardado.nombre}" actualizado.`
                : `"${guardado.nombre}" creado con ${guardado.variantes.length} ${guardado.variantes.length === 1 ? 'talla' : 'tallas y colores'}.`,
            }
      );
    } catch (e) {
      setMensaje({ tipo: 'error', texto: e.message });
    } finally {
      setGuardando(false);
    }
  };

  return (
    <div className="inv-modal-overlay" onClick={() => !guardando && onCerrar()}>
      <div className="inv-modal mod-modal" onClick={(e) => e.stopPropagation()} role="dialog" aria-modal="true" aria-label="Modelo con tallas">
        <h2>{editando ? 'Editar modelo' : 'Nuevo modelo con tallas'}</h2>
        <p className="mod-ayuda">
          Un modelo es la prenda (por ejemplo «Polo básico»). Cada talla y color lleva su propio precio, stock y código de barras.
        </p>

        <div className="inv-campo inv-campo-imagen">
          <label>Foto del modelo (opcional)</label>
          <div className="inv-imagen-selector">
            {imagenPreview ? (
              <img src={imagenPreview} alt="Vista previa" className="inv-imagen-preview" />
            ) : (
              <div className="inv-imagen-preview inv-imagen-preview-vacia">👕</div>
            )}
            <label className="inv-boton-subir-imagen">
              {imagenPreview ? 'Cambiar foto' : 'Elegir foto'}
              <input type="file" accept="image/*" onChange={manejarImagen} hidden />
            </label>
          </div>
        </div>

        <div className="inv-form-grid">
          <div className="inv-campo inv-campo-full">
            <label>Nombre del modelo</label>
            <input value={nombre} onChange={(e) => setNombre(e.target.value)} placeholder="Polo básico, Jean clásico, Zapatilla urbana..." maxLength={120} />
          </div>
          <div className="inv-campo">
            <label>Categoría</label>
            {nuevaCategoria === null ? (
              <>
                <select value={categoriaId} onChange={(e) => setCategoriaId(e.target.value)}>
                  <option value="">Selecciona...</option>
                  {listaCategorias.map((c) => (
                    <option key={c.id} value={c.id}>
                      {c.nombre}
                    </option>
                  ))}
                </select>
                <button type="button" className="inv-boton-nueva-categoria" onClick={() => setNuevaCategoria('')}>
                  + Nueva categoría
                </button>
              </>
            ) : (
              <div className="inv-nueva-categoria-fila">
                <input
                  type="text"
                  placeholder="Polos, Pantalones, Calzado..."
                  value={nuevaCategoria}
                  onChange={(e) => setNuevaCategoria(e.target.value)}
                  onKeyDown={(e) => {
                    if (e.key === 'Enter') {
                      e.preventDefault();
                      crearCategoria();
                    }
                  }}
                  maxLength={60}
                  autoFocus
                  aria-label="Nombre de la categoría nueva"
                />
                <button type="button" className="inv-boton-guardar-categoria" onClick={crearCategoria} disabled={creandoCategoria}>
                  {creandoCategoria ? '...' : 'Crear'}
                </button>
                <button type="button" className="inv-boton-cancelar-categoria" onClick={() => setNuevaCategoria(null)} aria-label="Cancelar categoría nueva">
                  ×
                </button>
              </div>
            )}
          </div>
          <div className="inv-campo">
            <label>Se vende por</label>
            <select value={unidad} onChange={(e) => setUnidad(e.target.value)}>
              {opcionesUnidad(unidadesActivas, unidad).map((u) => (
                <option key={u.valor} value={u.valor}>
                  {u.label}
                </option>
              ))}
            </select>
          </div>
          <div className="inv-campo">
            <label>{ganancias ? 'Precio de compra (S/, con IGV)' : 'Precio de compra (S/, opcional)'}</label>
            <input type="number" inputMode="decimal" value={precioCompra} onChange={(e) => setPrecioCompra(e.target.value)} />
            {ganancias && (
              <small className={`inv-ayuda-costo${aNumero(precioCompra) > 0 ? '' : ' falta'}`}>
                {aNumero(precioCompra) > 0
                  ? 'Vale para todas las tallas. Si a una talla le subes el stock, lo que entra se promedia a este precio.'
                  : 'Obligatorio: lo que te cuesta cada prenda, con IGV. Sin él no se puede calcular tu ganancia.'}
              </small>
            )}
          </div>
          <div className="inv-campo">
            <label>Avisar cuando queden (por talla)</label>
            <input type="number" inputMode="decimal" value={stockMinimo} onChange={(e) => setStockMinimo(e.target.value)} />
          </div>
        </div>

        <div className="mod-seccion">
          <span className="mod-titulo">1. Tallas</span>
          <div className="mod-chips">
            {tallas.map((t) => (
              <span key={t} className="mod-chip">
                {t}
                {!tieneExistentes('talla', t) && (
                  <button type="button" onClick={() => quitarTalla(t)} aria-label={`Quitar talla ${t}`}>
                    ×
                  </button>
                )}
              </span>
            ))}
            <input
              className="mod-chip-input"
              placeholder={tallas.length ? 'Otra talla' : 'Escribe: S, M, L'}
              value={textoTalla}
              onChange={(e) => setTextoTalla(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === 'Enter' || e.key === ',') {
                  e.preventDefault();
                  confirmarTexto(textoTalla, agregarTallas, setTextoTalla);
                }
              }}
              onBlur={() => confirmarTexto(textoTalla, agregarTallas, setTextoTalla)}
              aria-label="Agregar talla"
            />
          </div>
          <div className="mod-sugeridas">
            {TALLAS_SUGERIDAS.map((s) => (
              <button key={s.label} type="button" onClick={() => agregarTallas(s.tallas)}>
                {s.label}
              </button>
            ))}
          </div>
        </div>

        <div className="mod-seccion">
          <span className="mod-titulo">2. Colores (opcional)</span>
          <div className="mod-chips">
            {colores.map((c) => (
              <span key={c} className="mod-chip">
                {c}
                {!tieneExistentes('color', c) && (
                  <button type="button" onClick={() => quitarColor(c)} aria-label={`Quitar color ${c}`}>
                    ×
                  </button>
                )}
              </span>
            ))}
            <input
              className="mod-chip-input"
              placeholder={colores.length ? 'Otro color' : 'Escribe: Negro, Blanco'}
              value={textoColor}
              onChange={(e) => setTextoColor(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === 'Enter' || e.key === ',') {
                  e.preventDefault();
                  confirmarTexto(textoColor, agregarColores, setTextoColor);
                }
              }}
              onBlur={() => confirmarTexto(textoColor, agregarColores, setTextoColor)}
              aria-label="Agregar color"
            />
          </div>
        </div>

        {tallas.length > 0 && (
          <div className="mod-seccion">
            <span className="mod-titulo">3. Precio de cada talla (S/)</span>
            <div className="mod-precios">
              {tallas.map((t) => (
                <label key={t} className="mod-precio-talla">
                  <span>{t}</span>
                  <input
                    type="number"
                    inputMode="decimal"
                    min="0"
                    step="0.01"
                    placeholder="0.00"
                    value={precioDeTalla(t)}
                    onChange={(e) => fijarPrecioTalla(t, e.target.value)}
                    aria-label={`Precio de la talla ${t}`}
                  />
                </label>
              ))}
            </div>
          </div>
        )}

        {eligeColores && (
          <div className="mod-seccion">
            <span className="mod-titulo">4. ¿En qué colores viene cada talla?</span>
            <p className="mod-ayuda">Toca los colores de cada talla. Solo lo que marques se crea.</p>
            <div className="mod-colores-talla">
              {tallas.map((t) => {
                const deTalla = combinaciones.filter((c) => c.talla === t);
                const faltan = deTalla.some((c) => !filaDe(c).incluida);
                return (
                  <div key={t} className="mod-colores-fila">
                    <span className="mod-colores-nombre">{t}</span>
                    <div className="mod-colores-opciones">
                      {deTalla.map((c) => {
                        const fila = filaDe(c);
                        return (
                          <button
                            key={c.clave}
                            type="button"
                            className={`mod-color${fila.incluida ? ' activo' : ''}`}
                            onClick={() => alternarColor(c)}
                            aria-pressed={fila.incluida}
                            aria-label={`Talla ${t} en ${c.color}`}
                            title={fila.id ? 'Ya existe: se quita con la × de su fila' : undefined}
                          >
                            {fila.incluida ? '✓ ' : ''}
                            {c.color}
                          </button>
                        );
                      })}
                      {faltan && (
                        <button type="button" className="mod-color-todos" onClick={() => marcarTodos(t)} aria-label={`Todos los colores para la talla ${t}`}>
                          Todos
                        </button>
                      )}
                    </div>
                  </div>
                );
              })}
            </div>
          </div>
        )}

        {incluidas.length > 0 && (
          <div className="mod-seccion">
            <span className="mod-titulo">
              {eligeColores ? '5.' : tallas.length > 0 ? '4.' : '3.'} Stock y código de barras de cada una
            </span>
            <p className="mod-ayuda">
              Escanea o escribe el código de cada talla y color. Si lo dejas vacío, el sistema le pone uno.
            </p>
            <div className="mod-tabla" ref={tablaRef}>
              <div className="mod-fila mod-fila-cabecera">
                <span>Talla y color</span>
                <span>Precio</span>
                <span>Stock</span>
                <span>Código de barras</span>
                <span />
              </div>
              {incluidas.map((c) => {
                const fila = filaDe(c);
                const etiqueta = [c.talla, c.color].filter(Boolean).join(' · ');
                return (
                  <div key={c.clave} className="mod-fila">
                    <span className="mod-fila-nombre">
                      {etiqueta}
                      {!fila.id && editando && <em>nueva</em>}
                    </span>
                    <label>
                      <small>Precio</small>
                      <input
                        type="number"
                        inputMode="decimal"
                        min="0"
                        step="0.01"
                        placeholder="0.00"
                        value={fila.precio}
                        onChange={(e) => cambiarFila(c, { precio: e.target.value })}
                        aria-label={`Precio de ${etiqueta}`}
                      />
                    </label>
                    <label>
                      <small>Stock</small>
                      <input
                        type="number"
                        inputMode="decimal"
                        min="0"
                        placeholder="0"
                        value={fila.stock}
                        onChange={(e) => cambiarFila(c, { stock: e.target.value })}
                        aria-label={`Stock de ${etiqueta}`}
                      />
                    </label>
                    <label className="mod-fila-codigo">
                      <small>Código</small>
                      <span>
                        <input
                          data-codigo
                          placeholder="Escanear o escribir"
                          value={fila.codigo}
                          maxLength={60}
                          onChange={(e) => cambiarFila(c, { codigo: e.target.value })}
                          onKeyDown={alEnterEnCodigo}
                          aria-label={`Código de ${etiqueta}`}
                        />
                        <button type="button" className="inv-boton-escanear" onClick={() => setEscaneando(c)} aria-label={`Escanear código de ${etiqueta}`}>
                          📷
                        </button>
                      </span>
                    </label>
                    <button
                      type="button"
                      className="mod-boton-quitar"
                      onClick={() => (fila.id ? quitarExistente(c) : cambiarFila(c, { incluida: false }))}
                      aria-label={`Quitar ${etiqueta}`}
                      title={fila.id ? 'Eliminar o desactivar esta talla' : 'Quitar de la lista'}
                    >
                      ×
                    </button>
                  </div>
                );
              })}
            </div>
          </div>
        )}

        {mensaje && <p className={`inv-mensaje inv-mensaje-${mensaje.tipo}`}>{mensaje.texto}</p>}

        <div className="inv-modal-acciones">
          <button className="inv-boton-cancelar" onClick={onCerrar} disabled={guardando}>
            Cancelar
          </button>
          <button className="inv-boton-guardar" onClick={guardar} disabled={guardando}>
            {guardando
              ? 'Guardando...'
              : editando
                ? 'Guardar cambios'
                : incluidas.length > 0
                  ? `Crear modelo (${incluidas.length})`
                  : 'Crear modelo'}
          </button>
        </div>
      </div>

      {escaneando && (
        <div onClick={(e) => e.stopPropagation()}>
          <EscanerCodigoBarras
            cerrarAlDetectar
            onCodigoDetectado={(codigo) => cambiarFila(escaneando, { codigo })}
            onCerrar={() => setEscaneando(null)}
          />
        </div>
      )}
    </div>
  );
}
