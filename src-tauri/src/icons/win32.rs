//! Appels à l'API Windows (Win32) pour les icônes. Compilé uniquement sous Windows.
//!
//! # `unsafe` et FFI, en bref
//!
//! *FFI* (Foreign Function Interface) : appeler depuis Rust des fonctions écrites
//! dans un autre langage, ici les DLL de Windows écrites en C/C++. La crate
//! `windows` (de Microsoft) fournit leurs déclarations en Rust.
//!
//! Le compilateur Rust garantit qu'un programme ne lit jamais de mémoire libérée
//! ni hors d'un tableau… mais seulement pour le code Rust. Il ne peut rien
//! vérifier de ce que fait une fonction C. Ces appels doivent donc être écrits
//! dans un bloc `unsafe { … }` : on dit au compilateur « ici, c'est moi qui
//! garantis que les règles sont respectées ».
//!
//! `unsafe` ne désactive pas les vérifications du reste du code. C'est une
//! zone balisée, à garder petite et commentée : chaque bloc ci-dessous dit
//! *pourquoi* il est correct (les pointeurs passés sont valides, chaque
//! ressource Windows obtenue est rendue…).
//!
//! # Ressources Windows à rendre
//!
//! Une image Windows (`HBITMAP`), un contexte de dessin (`HDC`)… sont des
//! *handles* : des numéros qui désignent une ressource gérée par Windows. Rust
//! ne sait pas qu'il faut les rendre : on le fait à la main (`DeleteObject`,
//! `DeleteDC`). Les objets COM (`IShellItemImageFactory`), eux, sont rendus
//! automatiquement par la crate `windows` quand la variable disparaît (`Drop`).

use std::ffi::c_void;
use std::os::windows::ffi::OsStrExt;
use std::path::{Path, PathBuf};

use windows::Win32::Foundation::SIZE;
use windows::Win32::Graphics::Gdi::{
    BI_RGB, BITMAP, BITMAPINFO, BITMAPINFOHEADER, CreateCompatibleDC, DIB_RGB_COLORS, DeleteDC,
    DeleteObject, GetDIBits, GetObjectW, HBITMAP,
};
use windows::Win32::System::Com::{
    COINIT_APARTMENTTHREADED, COINIT_DISABLE_OLE1DDE, CoInitializeEx, CoUninitialize,
};
use windows::Win32::System::Registry::{HKEY_CURRENT_USER, RRF_RT_REG_SZ, RegGetValueW};
use windows::Win32::UI::Shell::{
    IShellItemImageFactory, SHCreateItemFromParsingName, SIIGBF_ICONONLY,
};
use windows::core::{PCWSTR, w};

/// Texte Rust → texte Windows (UTF-16 terminé par un zéro).
fn wide(path: &Path) -> Vec<u16> {
    path.as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect()
}

/// L'icône que l'Explorateur affiche pour `path` (exe, lnk, dll…), en pixels
/// BGRA de `size`×`size` au plus. Renvoie `(largeur, hauteur, pixels)`.
///
/// On passe par `IShellItemImageFactory`, l'interface qu'utilise l'Explorateur
/// lui-même. Elle sait suivre un raccourci `.lnk` jusqu'à sa cible, choisit la
/// meilleure taille dans le fichier (jusqu'à 256 px) et garde la transparence.
pub fn shell_image_bgra(path: &Path, size: i32) -> Result<(u32, u32, Vec<u8>), String> {
    let path_w = wide(path);

    // SAFETY (pourquoi ce bloc est correct) :
    // - `path_w` est un tableau UTF-16 terminé par 0 qui vit jusqu'à la fin de
    //   la fonction, donc pendant tout l'appel qui le lit ;
    // - COM est initialisé avant les appels COM et libéré après, seulement si
    //   notre appel l'a initialisé ;
    // - l'image obtenue (`HBITMAP`) est rendue avec `DeleteObject` dans tous
    //   les cas, après lecture de ses pixels.
    unsafe {
        // COM (le système d'objets de Windows) doit être initialisé sur le
        // thread qui l'utilise. « Apartment threaded » : le mode que demandent
        // les composants de l'Explorateur.
        let com = CoInitializeEx(None, COINIT_APARTMENTTHREADED | COINIT_DISABLE_OLE1DDE);

        // Closure appelée aussitôt : les `?` à l'intérieur en sortent, et
        // `factory` est détruit (rendu à COM) à la fin de la closure, *avant*
        // `CoUninitialize`. L'ordre compte : on ne libère pas COM sous un objet
        // encore vivant.
        let result = (|| -> windows::core::Result<_> {
            // `: IShellItemImageFactory` : la fonction est générique, le type
            // demandé dit à Windows quelle interface on veut sur l'élément.
            let factory: IShellItemImageFactory =
                SHCreateItemFromParsingName(PCWSTR(path_w.as_ptr()), None)?;
            // ICONONLY : l'icône, pas une miniature du contenu.
            let bitmap = factory.GetImage(SIZE { cx: size, cy: size }, SIIGBF_ICONONLY)?;
            let pixels = read_pixels(bitmap);
            // Le `HBITMAP` nous appartient : on le rend à Windows.
            let _ = DeleteObject(bitmap.into());
            Ok(pixels)
        })();

        if com.is_ok() {
            CoUninitialize();
        }
        // `e.message()` : le message d'erreur de Windows (« Fichier introuvable »…).
        result.map_err(|e| e.message())?
    }
}

/// Copie les pixels d'un `HBITMAP` dans un `Vec<u8>` (BGRA, ligne du haut d'abord).
///
/// `unsafe fn` : l'appelant doit garantir que `bitmap` est une image valide.
unsafe fn read_pixels(bitmap: HBITMAP) -> Result<(u32, u32, Vec<u8>), String> {
    // SAFETY : `bitmap` est valide (contrat de la fonction) ; chaque pointeur
    // passé désigne une variable locale assez grande pour ce que Windows y
    // écrit (`BITMAP`, `BITMAPINFO`, `buffer` de largeur × hauteur × 4 octets).
    unsafe {
        // 1. Taille de l'image.
        let mut info = BITMAP::default();
        let read = GetObjectW(
            bitmap.into(),
            std::mem::size_of::<BITMAP>() as i32,
            Some(&mut info as *mut BITMAP as *mut c_void),
        );
        if read == 0 || info.bmWidth <= 0 || info.bmHeight == 0 {
            return Err("image Windows illisible".into());
        }
        let width = info.bmWidth;
        let height = info.bmHeight.abs();

        // 2. Format voulu : 32 bits par pixel, sans compression. Une hauteur
        //    *négative* demande les lignes de haut en bas (sinon Windows les
        //    donne à l'envers, de bas en haut).
        let mut format = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: width,
                biHeight: -height,
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };

        // 3. Copie des pixels. `GetDIBits` a besoin d'un contexte de dessin
        //    (`HDC`) : on en crée un temporaire, qu'on rend juste après.
        let mut buffer = vec![0u8; width as usize * height as usize * 4];
        let dc = CreateCompatibleDC(None);
        let lines = GetDIBits(
            dc,
            bitmap,
            0,
            height as u32,
            Some(buffer.as_mut_ptr() as *mut c_void),
            &mut format,
            DIB_RGB_COLORS,
        );
        let _ = DeleteDC(dc);

        if lines != height {
            return Err("pixels de l'icône illisibles".into());
        }
        Ok((width as u32, height as u32, buffer))
    }
}

/// Dossier d'installation de Steam, d'après le registre
/// (`HKEY_CURRENT_USER\Software\Valve\Steam`, valeur `SteamPath`).
pub fn steam_path_from_registry() -> Option<PathBuf> {
    // 1024 caractères UTF-16 : largement assez pour un chemin.
    let mut buffer = vec![0u16; 1024];
    // `len` : taille du tableau en *octets* à l'entrée ; Windows y écrit la
    // taille réellement utilisée.
    let mut len = (buffer.len() * 2) as u32;

    // SAFETY : `w!("…")` produit un texte UTF-16 constant terminé par 0 ;
    // `buffer` fait bien `len` octets ; `len` est une variable locale.
    let status = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            w!("Software\\Valve\\Steam"),
            w!("SteamPath"),
            RRF_RT_REG_SZ,
            None,
            Some(buffer.as_mut_ptr() as *mut c_void),
            Some(&mut len),
        )
    };
    if status.is_err() {
        return None;
    }
    // `len` compte le zéro final : on garde ce qui le précède.
    let chars = (len as usize / 2).min(buffer.len());
    let end = buffer[..chars]
        .iter()
        .position(|&c| c == 0)
        .unwrap_or(chars);
    // Steam écrit des `/` (`c:/program files (x86)/steam`) : Windows les accepte.
    Some(PathBuf::from(String::from_utf16_lossy(&buffer[..end])))
}
